use serde::Deserialize;
use sha3::{Digest, Sha3_256};

use super::{ContentId, PluginError};

const MAX_NODES: usize = 128;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Fact {
    WorldHour,
    WorldMinute,
    Season,
    Weather,
    SurfaceDry,
    AllAvailable,
    UrgentNeeds,
    Willing,
    EssentialWorkDue,
    FacilityUsable(ContentId),
    ItemObtainable(ContentId),
    CooldownReady,
    LastSucceeded,
    HasWish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactValue {
    Bool(bool),
    Number(u16),
    Season(Season),
    Weather(Weather),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Weather {
    Clear,
    Clouds,
    Rain,
    Snow,
}

pub trait FactSource {
    /// Return None when the fact cannot be determined from world state.
    fn read(&self, fact: &Fact) -> Option<FactValue>;
}

impl<F: Fn(&Fact) -> Option<FactValue>> FactSource for F {
    fn read(&self, fact: &Fact) -> Option<FactValue> {
        self(fact)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Expression {
    all: Option<Vec<Expression>>,
    any: Option<Vec<Expression>>,
    not: Option<Box<Expression>>,
    fact: Option<FactName>,
    #[serde(rename = "ref")]
    reference: Option<ContentId>,
    eq: Option<Literal>,
    in_range: Option<[u16; 2]>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
enum FactName {
    #[serde(rename = "world.hour")]
    WorldHour,
    #[serde(rename = "world.minute")]
    WorldMinute,
    #[serde(rename = "world.season")]
    Season,
    #[serde(rename = "weather.kind")]
    Weather,
    #[serde(rename = "weather.surface_dry")]
    SurfaceDry,
    #[serde(rename = "participants.all_available")]
    AllAvailable,
    #[serde(rename = "participants.urgent_needs")]
    UrgentNeeds,
    #[serde(rename = "participants.willing")]
    Willing,
    #[serde(rename = "household.essential_work_due")]
    EssentialWorkDue,
    #[serde(rename = "facility.usable")]
    FacilityUsable,
    #[serde(rename = "item.obtainable")]
    ItemObtainable,
    #[serde(rename = "cooldown.ready")]
    CooldownReady,
    #[serde(rename = "result.last_succeeded")]
    LastSucceeded,
    #[serde(rename = "household.has_wish")]
    HasWish,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum Literal {
    Boolean(bool),
    Number(u16),
    Word(String),
}

#[derive(Clone, Debug)]
enum Comparison {
    Equal(FactValue),
    Range([u16; 2]),
}

#[derive(Clone, Debug)]
enum Instruction {
    All { end: u8 },
    Any { end: u8 },
    Not,
    Test(Fact, Comparison),
}

#[derive(Clone, Debug)]
pub struct CompiledCondition {
    instructions: Box<[Instruction]>,
}

impl CompiledCondition {
    pub fn from_json(input: &[u8]) -> Result<Self, PluginError> {
        if input.len() > 64 * 1024 {
            return Err(PluginError::Capacity("条件文件大小"));
        }
        let expression: Expression = serde_json::from_slice(input)
            .map_err(|err| PluginError::invalid(format!("条件 JSON：{err}")))?;
        expression.compile()
    }

    pub fn evaluate(&self, facts: &impl FactSource) -> bool {
        self.evaluate_at(facts, &mut 0).unwrap_or(false)
    }

    pub fn facts(&self) -> impl Iterator<Item = &Fact> {
        self.instructions.iter().filter_map(|instruction| {
            if let Instruction::Test(fact, _) = instruction {
                return Some(fact);
            }
            None
        })
    }

    pub(super) fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(&*self.instructions)
            + self
                .facts()
                .map(|fact| match fact {
                    Fact::FacilityUsable(id) | Fact::ItemObtainable(id) => id.as_str().len(),
                    _ => 0,
                })
                .sum::<usize>()
    }

    fn evaluate_at(&self, facts: &impl FactSource, position: &mut usize) -> Option<bool> {
        let instruction = &self.instructions[*position];
        *position += 1;
        match instruction {
            Instruction::Not => self.evaluate_at(facts, position).map(|value| !value),
            Instruction::Test(fact, comparison) => match (facts.read(fact)?, comparison) {
                (value, Comparison::Equal(expected))
                    if std::mem::discriminant(&value) == std::mem::discriminant(expected) =>
                {
                    Some(value == *expected)
                }
                (FactValue::Number(value), Comparison::Range([start, end])) => {
                    Some((*start..*end).contains(&value))
                }
                _ => None,
            },
            Instruction::All { end } | Instruction::Any { end } => {
                let end = usize::from(*end);
                let decisive = matches!(instruction, Instruction::Any { .. });
                let mut unknown = false;
                while *position < end {
                    match self.evaluate_at(facts, position) {
                        Some(result) if result == decisive => {
                            *position = end;
                            return Some(decisive);
                        }
                        None => unknown = true,
                        _ => {}
                    }
                }
                (!unknown).then_some(!decisive)
            }
        }
    }
}

impl Expression {
    pub(super) fn compile(self) -> Result<CompiledCondition, PluginError> {
        let mut instructions = Vec::new();
        self.emit(&mut instructions, 1)?;
        Ok(CompiledCondition {
            instructions: instructions.into_boxed_slice(),
        })
    }

    fn emit(self, instructions: &mut Vec<Instruction>, depth: usize) -> Result<(), PluginError> {
        if depth > MAX_DEPTH || instructions.len() >= MAX_NODES {
            return Err(PluginError::Capacity("条件节点数量或深度"));
        }
        let contains_test =
            self.reference.is_some() || self.eq.is_some() || self.in_range.is_some();
        let choices = [
            self.all.is_some(),
            self.any.is_some(),
            self.not.is_some(),
            self.fact.is_some(),
        ]
        .into_iter()
        .filter(|&choice| choice)
        .count();
        if choices != 1 || (contains_test && self.fact.is_none()) {
            return Err(PluginError::invalid("条件节点必须包含一种运算"));
        }
        if let Some(name) = self.fact {
            let (fact, comparison) = name.compile_test(self.reference, self.eq, self.in_range)?;
            instructions.push(Instruction::Test(fact, comparison));
            return Ok(());
        }
        if let Some(child) = self.not {
            instructions.push(Instruction::Not);
            return child.emit(instructions, depth + 1);
        }
        let all = self.all.is_some();
        let children = self.all.or(self.any).expect("one logical operator");
        if children.is_empty() {
            return Err(PluginError::invalid("all/any 条件至少需要一个子项"));
        }
        let start = instructions.len();
        instructions.push(Instruction::All { end: 0 });
        for child in children {
            child.emit(instructions, depth + 1)?;
        }
        let end = instructions.len() as u8;
        instructions[start] = if all {
            Instruction::All { end }
        } else {
            Instruction::Any { end }
        };
        Ok(())
    }
}

impl FactName {
    fn compile_test(
        self,
        reference: Option<ContentId>,
        eq: Option<Literal>,
        range: Option<[u16; 2]>,
    ) -> Result<(Fact, Comparison), PluginError> {
        let fact = match (self, reference) {
            (Self::FacilityUsable, Some(id)) => Fact::FacilityUsable(id),
            (Self::ItemObtainable, Some(id)) => Fact::ItemObtainable(id),
            (Self::FacilityUsable | Self::ItemObtainable, None) => {
                return Err(PluginError::invalid("设施或物品条件缺少 ref"));
            }
            (_, Some(_)) => return Err(PluginError::invalid("此条件不支持 ref")),
            (Self::WorldHour, None) => Fact::WorldHour,
            (Self::WorldMinute, None) => Fact::WorldMinute,
            (Self::Season, None) => Fact::Season,
            (Self::Weather, None) => Fact::Weather,
            (Self::SurfaceDry, None) => Fact::SurfaceDry,
            (Self::AllAvailable, None) => Fact::AllAvailable,
            (Self::UrgentNeeds, None) => Fact::UrgentNeeds,
            (Self::Willing, None) => Fact::Willing,
            (Self::EssentialWorkDue, None) => Fact::EssentialWorkDue,
            (Self::CooldownReady, None) => Fact::CooldownReady,
            (Self::LastSucceeded, None) => Fact::LastSucceeded,
            (Self::HasWish, None) => Fact::HasWish,
        };
        let exclusive_max = match fact {
            Fact::WorldHour => Some(24),
            Fact::WorldMinute => Some(1440),
            _ => None,
        };
        let comparison = match (eq, range) {
            (Some(Literal::Number(value)), None)
                if exclusive_max.is_some_and(|max| value < max) =>
            {
                Comparison::Equal(FactValue::Number(value))
            }
            (None, Some([start, end]))
                if start < end && exclusive_max.is_some_and(|max| end <= max) =>
            {
                Comparison::Range([start, end])
            }
            (Some(Literal::Word(word)), None) if matches!(fact, Fact::Season) => {
                let value = match word.as_str() {
                    "spring" => Season::Spring,
                    "summer" => Season::Summer,
                    "autumn" => Season::Autumn,
                    "winter" => Season::Winter,
                    _ => return Err(PluginError::invalid(format!("未知季节 {word:?}"))),
                };
                Comparison::Equal(FactValue::Season(value))
            }
            (Some(Literal::Word(word)), None) if matches!(fact, Fact::Weather) => {
                let value = match word.as_str() {
                    "clear" => Weather::Clear,
                    "clouds" => Weather::Clouds,
                    "rain" => Weather::Rain,
                    "snow" => Weather::Snow,
                    _ => return Err(PluginError::invalid(format!("未知天气 {word:?}"))),
                };
                Comparison::Equal(FactValue::Weather(value))
            }
            (Some(Literal::Boolean(value)), None)
                if exclusive_max.is_none() && !matches!(fact, Fact::Weather | Fact::Season) =>
            {
                Comparison::Equal(FactValue::Bool(value))
            }
            _ => {
                return Err(PluginError::invalid(format!(
                    "条件 {self:?} 比较类型或范围无效"
                )));
            }
        };
        Ok((fact, comparison))
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chance {
    pub numerator: u32,
    pub denominator: u32,
    pub window: Window,
    pub draw_at_first_eligible_opportunity_only: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    WorldDay,
    WorldDayAfternoon,
}

impl Chance {
    pub(super) fn validate(self) -> Result<(), PluginError> {
        if self.denominator == 0
            || self.denominator > 1_000_000
            || self.numerator > self.denominator
            || !self.draw_at_first_eligible_opportunity_only
        {
            return Err(PluginError::invalid("概率分数或决定窗口无效"));
        }
        Ok(())
    }

    /// The host supplies the ID of the stable window, usually a world day.
    /// Repeated observations of that window produce the same result.
    pub fn allows(self, world_seed: u64, activity: &ContentId, window_id: u64) -> bool {
        if self.validate().is_err() || self.numerator == 0 {
            return false;
        }
        if self.numerator == self.denominator {
            return true;
        }
        let mut hash = Sha3_256::new();
        hash.update(b"cat-home:activity-proposal:v1\0");
        hash.update(world_seed.to_le_bytes());
        hash.update(window_id.to_le_bytes());
        hash.update([match self.window {
            Window::WorldDay => 0,
            Window::WorldDayAfternoon => 1,
        }]);
        hash.update(activity.as_str().as_bytes());
        let result = hash.finalize();
        let roll = u64::from_le_bytes(result[..8].try_into().expect("hash prefix"));
        u128::from(roll) * u128::from(self.denominator) < u128::from(self.numerator) << 64
    }
}
