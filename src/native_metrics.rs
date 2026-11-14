//! Opt-in local measurement of the actual native process, never telemetry.
use std::{io, mem, path::PathBuf, time::Instant};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
        Threading::{GetCurrentProcess, GetGuiResources, GetProcessTimes},
    },
};
#[derive(serde::Serialize)]
struct Sample {
    elapsed_ms: u64,
    cpu_ms: f64,
    working_set_bytes: usize,
    private_bytes: usize,
    gdi_objects: u32,
}

const FRAME_INTERVAL_LIMITS_MS: [u64; 8] = [8, 12, 20, 28, 40, 64, 128, u64::MAX];

/// A fixed-size summary: enabling frame measurements never appends a row per frame.
#[derive(Default, serde::Serialize)]
struct FrameCadence {
    active_intervals: u64,
    active_elapsed_ms: u64,
    active_interval_buckets: [u64; 8],
    active_max_interval_ms: u64,
    quiet_intervals: u64,
    quiet_elapsed_ms: u64,
    camera_steps: u64,
    camera_max_axis_step_px: u64,
    camera_steps_over_24px: u64,
    #[serde(skip)]
    previous: Option<(u64, (i32, i32), bool)>,
}

impl FrameCadence {
    fn record(&mut self, elapsed_ms: u64, camera: (i32, i32), active: bool) {
        let previous = self.previous.replace((elapsed_ms, camera, active));
        let Some((before_ms, before_camera, before_active)) = previous else { return };
        let dt = elapsed_ms.saturating_sub(before_ms);
        if dt == 0 { return; }
        // Do not count the idle-to-motion wake-up as a slow animation frame.
        if active && before_active {
            self.active_intervals = self.active_intervals.saturating_add(1);
            self.active_elapsed_ms = self.active_elapsed_ms.saturating_add(dt);
            let bucket = FRAME_INTERVAL_LIMITS_MS.iter().position(|limit| dt <= *limit).unwrap();
            self.active_interval_buckets[bucket] = self.active_interval_buckets[bucket].saturating_add(1);
            self.active_max_interval_ms = self.active_max_interval_ms.max(dt);
        } else if !active && !before_active {
            self.quiet_intervals = self.quiet_intervals.saturating_add(1);
            self.quiet_elapsed_ms = self.quiet_elapsed_ms.saturating_add(dt);
        }
        let step = (i64::from(camera.0) - i64::from(before_camera.0)).unsigned_abs()
            .max((i64::from(camera.1) - i64::from(before_camera.1)).unsigned_abs());
        if step > 0 {
            self.camera_steps = self.camera_steps.saturating_add(1);
            self.camera_max_axis_step_px = self.camera_max_axis_step_px.max(step);
            self.camera_steps_over_24px = self.camera_steps_over_24px.saturating_add(u64::from(step > 24));
        }
    }
}
fn sample(start: Instant) -> io::Result<Sample> {
    unsafe {
        let process = GetCurrentProcess();
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = mem::zeroed();
        counters.cb = mem::size_of_val(&counters) as u32;
        if GetProcessMemoryInfo(
            process,
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            counters.cb,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let (mut create, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
            (mem::zeroed(), mem::zeroed(), mem::zeroed(), mem::zeroed());
        if GetProcessTimes(process, &mut create, &mut exit, &mut kernel, &mut user) == 0 {
            return Err(io::Error::last_os_error());
        }
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        Ok(Sample {
            elapsed_ms: start.elapsed().as_millis() as u64,
            cpu_ms: (ticks(kernel) + ticks(user)) as f64 / 10_000.0,
            working_set_bytes: counters.WorkingSetSize,
            private_bytes: counters.PrivateUsage,
            gdi_objects: GetGuiResources(process, 0),
        })
    }
}
pub struct NativeMetrics {
    started: Instant,
    duration_ms: u64,
    next_ms: u64,
    initial: Sample,
    samples: Vec<Sample>,
    destination: PathBuf,
    done: bool,
    first_world_ms: Option<u64>,
    latest_world_ms: Option<u64>,
    rendered_frames: u64,
    cadence: FrameCadence,
}
impl NativeMetrics {
    pub fn new(seconds: u64) -> io::Result<Self> {
        let started = Instant::now();
        Ok(Self {
            started,
            duration_ms: seconds.clamp(5, 300) * 1000,
            next_ms: 1000,
            initial: sample(started)?,
            samples: Vec::with_capacity(300),
            destination: std::env::current_exe()?.with_file_name("native-metrics.json"),
            done: false,
            first_world_ms: None,
            latest_world_ms: None,
            rendered_frames: 0,
            cadence: FrameCadence::default(),
        })
    }
    /// Verify that a low CPU sample still contains live simulation and drawing.
    pub fn record_world_frame(&mut self, world_ms: u64) {
        self.first_world_ms.get_or_insert(world_ms);
        self.latest_world_ms = Some(world_ms);
        self.rendered_frames = self.rendered_frames.saturating_add(1);
    }
    /// Coordinates are the actual integer screen-pixel camera origin used to draw.
    /// `active` describes visible motion/high-cadence rendering, not distant actors.
    pub fn record_camera_frame(&mut self, camera_px: (i32, i32), active: bool) {
        self.cadence.record(self.started.elapsed().as_millis() as u64, camera_px, active);
    }
    /// Only enabled with --measure-seconds; one bounded report is overwritten.
    pub fn update(&mut self, size: (i32, i32)) -> io::Result<bool> {
        if self.done {
            return Ok(false);
        }
        let now = self.started.elapsed().as_millis() as u64;
        if now < self.next_ms {
            return Ok(false);
        }
        self.next_ms = now + 1000;
        self.samples.push(sample(self.started)?);
        if now < self.duration_ms {
            return Ok(false);
        }
        let last = self.samples.last().unwrap();
        let peak_working = self
            .samples
            .iter()
            .map(|s| s.working_set_bytes)
            .max()
            .unwrap();
        let peak_private = self.samples.iter().map(|s| s.private_bytes).max().unwrap();
        let report = serde_json::json!({"scope":"actual Win32 process; CPU is percent of one logical core; GPU/compositor not measured",
            "physical_size":[size.0,size.1],"elapsed_ms":last.elapsed_ms,"cpu_one_core_percent":(last.cpu_ms-self.initial.cpu_ms)/last.elapsed_ms.max(1) as f64*100.0,
            "peak_sampled_working_set_bytes":peak_working,"peak_sampled_private_bytes":peak_private,
            "world_start_ms":self.first_world_ms,"world_end_ms":self.latest_world_ms,"rendered_frames":self.rendered_frames,
            "initial":self.initial,"samples":self.samples,
            "frame_cadence_scope":"render-call intervals during visible motion, not GPU presentation timing; transition intervals excluded; camera cuts/manual pans included in step counters",
            "active_render_hz":self.cadence.active_intervals as f64 * 1000.0 / self.cadence.active_elapsed_ms.max(1) as f64,
            "quiet_render_hz":self.cadence.quiet_intervals as f64 * 1000.0 / self.cadence.quiet_elapsed_ms.max(1) as f64,
            "frame_interval_bucket_labels_ms":["<=8","9-12","13-20","21-28","29-40","41-64","65-128",">128"],
            "frame_cadence":self.cadence});
        std::fs::write(&self.destination, serde_json::to_vec_pretty(&report)?)?;
        self.done = true;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_separates_idle_wakeup_from_sustained_motion() {
        let mut stats = FrameCadence::default();
        stats.record(0, (0, 0), false);
        stats.record(100, (0, 0), false);
        stats.record(200, (1, 0), true);
        stats.record(216, (2, 0), true);
        stats.record(232, (3, 0), true);
        stats.record(332, (3, 0), false);
        stats.record(432, (3, 0), false);
        assert_eq!((stats.active_intervals, stats.active_elapsed_ms), (2, 32));
        assert_eq!((stats.quiet_intervals, stats.quiet_elapsed_ms), (2, 200));
        assert_eq!(stats.active_interval_buckets[2], 2);
        assert_eq!(stats.camera_max_axis_step_px, 1);
        assert_eq!(stats.camera_steps_over_24px, 0);
    }

    #[test]
    fn cadence_tracks_stalls_and_camera_cuts_without_frame_history() {
        let mut stats = FrameCadence::default();
        stats.record(0, (0, 0), true);
        stats.record(200, (100, 0), true);
        stats.record(200, (100, 0), true);
        for i in 1..=100_000 { stats.record(200 + i * 16, (100, 0), true); }
        assert_eq!(stats.active_intervals, 100_001);
        assert_eq!(stats.active_interval_buckets[7], 1);
        assert_eq!(stats.active_max_interval_ms, 200);
        assert_eq!(stats.camera_steps_over_24px, 1);
        assert!(serde_json::to_vec(&stats).unwrap().len() < 512);
    }
}
