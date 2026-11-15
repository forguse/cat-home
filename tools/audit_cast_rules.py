"""Audit the historical cast catalogue against the normal 1.1.4 village."""

from __future__ import annotations

import argparse
from collections import Counter
import csv
from dataclasses import dataclass
import io
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
DESIGN = ROOT / "design"
BLUEPRINT = ROOT / "assets/maps/willow-village-192x128-r1.json"
BINDINGS = ROOT / "assets/maps/village-legacy-bindings.tsv"
OUTPUT = ROOT / "docs/legacy-cast-rule-audit.tsv"
CLIMATES = ("dawn", "day", "dusk", "night"), ("clear", "clouds", "rain", "snow"), ("spring", "autumn", "winter")
PLACEMENTS = ("就近", "前院地块", "扩建地块", "目标左侧", "沿目标正面", "目标右侧")
DISCLOSURE = "旧 ID 未绑定正式村庄"
UNBOUND = "同核未绑定"
READY = "可小规模接入"
ADAPT = "有局部机制需改编"
NEEDS = "缺所需机制"
ACTOR = "缺角色或名单不匹配"
CONTEXT = "原条件不可达"
MODE = "变体上下文待绑定"

# Mapping is a capability selection, not an ID alias. The florist, fruit
# grower and apprentice only substitute wherever their actual duty fits.
PEOPLE = {
    "H01": "xiaohe/ayu（按真实工作分工选其一）",
    "H02": "florist/fruit_grower/farmer_apprentice（分拆园艺职责）",
    "H03": "carpenter", "H04": "", "H05": "", "H06": "cook",
    "H07": "mail_carrier", "H08": "irrigator", "H09": "traveling_painter", "H10": "seed_merchant",
}
ANIMALS = (
    "orange", "grey_cat", "black_cat", "sparrow", "squirrel", "hedgehog", "rabbit", "frog",
    "swallow", "duckling", "snail", "ladybird", "firefly", "honeybee", "crow", "great_tit",
    "butterfly", "lizard", "field_mouse", "white_pigeon",
)
ROLES = PEOPLE | {f"A{i:02d}": name for i, name in enumerate(ANIMALS, 1)}


@dataclass(frozen=True)
class Mechanism:
    evidence: str
    limitation: str


EFFECTS = {
    "Trail": Mechanism("footprints.rs::Tracks / living_world.rs::record_footstep",
                       "仅两人和橘子实踏湿地或浅雪；动物习惯路线、方向引导和旧三种时效没有通用记录。"),
    "Signal": Mechanism("residents.rs::service_commit / village-services.tsv 公告",
                        "能现场发布或读取公告；通用听者感知、动物信号和惊响半径缺失。"),
    "Rest": Mechanism("living_world.rs::Task::Rest / Sleep",
                      "家庭实地休息按需要恢复；没用旧短停标记，不记录所有角色的常用落脚点。"),
    "Bond": Mechanism("residents_services.rs::build_rapport / social_story.rs Q36/Q71",
                      "有现场互访与关系值；通用双人搭伴、动物关系及最后相遇位置缺失。"),
    "MoveObject": Mechanism("residents_services.rs::ServiceLedger / 34 种实物移动服务",
                            "仅已登记对象有来源、归属、拾取、交接、退回；野果、小石、瓶盖等需登记实物。"),
    "Forage": Mechanism("living_world.rs::commit_scenic / Fish；residents.rs ME080",
                        "家庭钓鱼与资源入库、松鼠藏果有独立约束；野生通用食物/猎物/缓存模型缺失。"),
    "Tidy": Mechanism("ME003 / living_world.rs::CommunityAction::CleanCatBowl / road_story.rs Q59",
                      "确有特定实物清洗或收场；没有统一垃圾、移痕、旧工具污渍或动态落叶台账。"),
    "Water": Mechanism("ME002/ME014 / living_world.rs::commit_plot",
                       "井水、碗与田畦分别守恒；不同水圈、液位、灌口和野生补水未通用。"),
    "Drain": Mechanism("RI162/ME015 / soil.rs / road_story.rs MarketWork::Drain",
                       "田畦已有积水标志和指定排水工序；全图水位、临时浅槽和坡面水流缺失。"),
    "Sow": Mechanism("ME013/ME016 / living_world.rs::commit_plot",
                     "翻土后现场耗一份种子；混合花种、嫁接、野生撒种和植物多身份未支持。"),
    "TendPlant": Mechanism("RI101/RI103 / living_world.rs::commit_plot / orchard states",
                           "指定田与果园可照料；支架、覆盖、树苗单枝和任意花卉生长进度缺栏。"),
    "Pollinate": Mechanism("ME075/RI066 / residents.rs::observe_wildlife / orchard fruit trial",
                           "蜂访可给真实果园提供有限加成；没有单花传粉位置、惯常路径或一般种子增益。"),
    "Harvest": Mechanism("ME017/ME027 / living_world.rs::commit_plot / ProcessHarvest",
                         "成熟菜实际收进篮和留种；花种产品、特定果样/色泽和两个物理篮位需单独处理。"),
    "Build": Mechanism("ME039/ME040 / living_world.rs::development_commit / housing.rs",
                       "真实量测、行程、付款和特定工程；没用旧任意对象加分钟，温室等对象不存在。"),
    "Repair": Mechanism("RI021/ME006 / living_world.rs::apply_community",
                        "农具柄、门闩等有指定料和位置；风损、车辆、工具箱盖和逐块损坏状态缺失。"),
    "Shelter": Mechanism("social_story.rs Q50 / ME046 / ME078 / ME082",
                         "有特定雨棚、推迟和鸟类避雨；通用遮护状态与被打断目标恢复缺失。"),
    "Nest": Mechanism("RI151/RI173 栖地保护；residents.rs 动物观察路线",
                      "观景栖点不能代表巢；缺可占用巢、巢料、可持续繁殖阶段及育幼。"),
    "Deliver": Mechanism("RI001/RI002/RI005 / residents_services.rs 真实物件与材料来源",
                         "对象到场实物交接；没有完整通用邮包改址、耐雨封装或同程多件处置。"),
    "Forecast": Mechanism("living_world.rs::weather_at / RI004/RI069 现场公告",
                          "读取当下天气和固定窗有证明；需要预报期限、仪器读数与任务消费。"),
    "Art": Mechanism("RI081/RI083/RI086 / social_story.rs Q69",
                     "有画图服务和真实会堂布置；不同画作实体、颜料/纸和具体技法未齐。"),
    "SeedTrade": Mechanism("RI014/RI019/ME049/ME051/ME056 / residents.rs::offer_trip",
                           "摊主和家庭真实交易、收摊退物；没有旧拾物货币、分类种子柜或季节种身份。"),
    "Food": Mechanism("RI031/RI032/RI034 / living_world.rs::Task::Cook / ServeCat",
                      "真实耗材、份量、厨房与餐桌；不同菜谱、特定餐具及按旧效果加食物不可互换。"),
    "Lamp": Mechanism("road_story.rs Q57 / living_world.rs::market_lamp_tiles",
                      "公共灯实装及验收；提灯、可调门灯和通用听风风场没有。"),
    "Survey": Mechanism("RI007/RI093/RI167/ME041/ME084 / workshop_story.rs",
                        "有具体地点/工程/库存查验；缺通用对象识别记忆、景物状态字段和所有记录物件。"),
}

SITES = {
    "porch": "home_xiaohe.door/home_ayu.door",
    "path": "home_gate/home_lane",
    "mat": "cat_rest",
    "fence": "home_gate（固定边界，非可修栅栏）",
    "water": "home_well/cat_water",
    "sky": "无通用飞行地籍",
    "mailbox": "home_trade/craft_post.door",
    "pantry": "pantry_loading/home_pantry.door",
    "lamp": "market_lamp_tiles（运行时固定设施）",
    "flower": "commons_garden/south_apiary",
    "tree": "forest_loading/orchard_tools",
    "crop": "crop_00/crop_06（及本田其他已启用床）",
    "build": "现有工程工作脚点/住房地籍",
    "damage": "无旧灾害地籍；只映射真实洪涝或明确损坏对象",
    "garden": "commons_garden/家田",
    "pond": "home_fishing",
}


@dataclass(frozen=True)
class Reference:
    status: str
    site: str
    evidence: str
    note: str
    visual: str = "原生动作可复用；原独有构件仍按备注"


def example(site, evidence, note):
    return Reference(UNBOUND, site, evidence, note)


def candidate(site, evidence, note, visual="可复用通用步行或作业姿态；核对指定物件"):
    return Reference(READY, site, evidence, note, visual)


# Individual reviewed cases. Functional matches require a physical outcome,
# and still carry the old actor, trigger or presentation delta in the note.
REFERENCES = {
    "H01-13": example("crop_00、crop_06 或复用田", "Task::Clear/Till；RI101",
                      "采后地块返回可耕状态，真实翻地；不声称额外细沟是独立持久物。"),
    "H01-17": example("原畦作业脚→粮仓或农储屋", "Task::Harvest → ProcessHarvest → Store",
                      "菜和留种都实际入账；现代合用一篮并携留种，缺第二篮专属画面。"),
    "H01-19": candidate("现有施工限界工作脚", "ME039；DevelopmentKind::Road 的丈量阶段",
                        "跨工程注册一次测量收据；绑定有效工程边界，不能给不存在的工位放线。"),
    "H01-20": example("实际住房工地门外", "Task::BuildHome / HousingPhase::Frame",
                      "开工后携已扣实木到场，框架实际升级；没有单根梁件库存。"),
    "H01-26": example("具有洪涝记录的 crop_24 或原出水脚", "RI162 / ME015 / Task::Drain",
                      "目标实际积水才能清除；原石径旁水洼位置需要选择已登记出口。"),
    "H01-27": candidate("home_workshop.door", "Task::ReturnTool / RI107",
                        "用已借工具的唯一归还点，需有限泥渍状态；不能只换已归还标签。"),
    "H01-28": example("home_lane 的命名修补点", "RI092 / road_story.rs 实际铺路",
                      "固定修补可表现投料和验收；新增松动对象须绑定已标记的踏石。"),
    "H01-29": example("home_fishing", "Task::Fish / fishing_result",
                      "角色先从架上拿钓竿，再到水边抛线。旧 Survey 结果词仅保留为记录意图。"),
    "H01-30": example("home_fishing→pantry_loading", "Task::Fish / Store / ReturnTool",
                      "真实鱼获、空钩和竿归还分支已存在；该旧基础规则无条件变体。"),
    "H01-33": example("缺水的命名家田", "Task::Water / commit_plot",
                      "实际扣用水并推进该畦生长；只为已识别田畦补水，不凭视觉干斑加水。"),
    "H01-34": candidate("有积水的家田；既有排水实验出口", "ME015 / soil.rs ClearDrain / TestDrain",
                        "可复用排出积水；若作者要求可走浅槽，就仍要另建真实几何状态。"),
    "H01-37": example("粮柜→cat_water", "Task::ServeCat / CatEat",
                      "按一份实扣餐送到猫碗并消费；没有独立秤装饰动画。"),
    "H01-38": candidate("home_well→cat_water", "ME003 CleanCatBowl + ME002 FillCatBowl",
                        "补净水与清碗有真实水量。若实现全碗换水，旧水须先倒掉入有归属废水，不能免费替换。"),
    "H01-43": candidate("原有餐桌或 home_trade 旁安全小桌", "RI032 / social_story.rs Q35",
                        "可用真实分餐和预约，单列门外点心位与时限；来客未到须原物返还。"),
    "H01-46": candidate("home_workshop.door 双人有效站位", "Q36–Q39 / ME004 / workshop_story.rs 木件订单",
                        "实际预约同一良材批次与工段；此旧双人进度核对未注册。"),
    "H01-48": example("home_trade", "RI001 / RI002",
                      "移交真实原信或回信，原物可追溯；落点由门前互站位映射。"),
    "H01-52": candidate("home_ayu.door/干燥田道", "Task::ChangeClothes / Outfit::Raincoat",
                        "只有本人确实回衣柜换衣且可走湿地边才启动，脚印另依真实地面。"),
    "H01-53": candidate("近院踏脚位或 courtyard corner", "Task::Rest / Task::CourtyardBreak",
                        "可复用真实休息；选定衣柜已有服装与真实落脚位，具体针织样式待验。"),
    "H01-60": example("对应本人当前住房门", "Task::Sleep / home_door",
                      "独立作息与家庭住房真实归属；已有旧个人房或共享房时去各自实际门。"),
    "H03-01": candidate("真实未施工地；future lot 预留入口", "ME039 / ME084 / 建设丈量",
                        "量测有既有状态，但木匠不能接管别人正在支付的地籍；需独立预约。"),
    "H03-02": candidate("home_workshop.door", "RI022 / Q26–Q27 / workshop_story.rs",
                        "铺开实际带来的木料；定尺件须明确容量与订单，禁止通用木料重复抵扣。"),
    "H03-10": candidate("home_workshop.door / 已结束的施工收料脚", "RI030 / ServiceLedger",
                        "只回收真实有归属的余料，完工时实际把物料托管到有限货架。"),
    "H06-01": candidate("home_kitchen.door→cat_water", "Task::ServeCat / cook 的真实配餐",
                        "具名厨师需要新增到猫碗的真实托管路径；禁止到场只加饭量。"),
    "H06-03": candidate("干燥来客桌位", "RI032 / Q50 檐下试摊",
                        "库存实际分餐后准备到位，场地条件和物资归属还需逐项接线。"),
    "H06-05": candidate("home_kitchen.door→已有堆肥施工点", "soil.rs::scraps / Q21",
                        "现有厨房和堆肥状态可复用；要登记一份真实厨余交接，不能增加可吃食物。"),
    "H06-07": candidate("home_kitchen.door/到场约客门", "RI032/RI036 / 服务材料托管",
                        "以预约对象和实际厨食为准；取消转入原有退回路径。"),
    "H06-09": candidate("stall_meal 的公告点", "RI037 / RI004 / 真实柜台库存",
                        "挂营业牌须引用真实已上架库存，不虚构当日供应。"),
    "H07-01": example("home_trade", "RI002 / Move 服务",
                      "可收同一封有章回信；旧黎明时段要重新确认居民到场。"),
    "H07-03": candidate("home_trade", "RI001/RI004 / 真实信件交接",
                        "源信/收信人已具备；天气急札须绑定实际事件凭证，不宣称有预测。"),
    "H07-05": candidate("home_trade / craft_post.door", "RI001 / RI006 / 变更公告状态",
                        "为该原件添加限一到场确认；邮戳可用共享道具。"),
    "H07-06": candidate("原投递点与原寄件人", "RI005 / 真实预约失约与退回",
                        "同一凭条按预约截止可实际退还，不生成另一封信。"),
    "H07-08": candidate("原送件点→开放的雨棚", "RI003/RI010 / Q50",
                        "按现实天气和既有实体防雨处改约，保留来源与原承接人。"),
    "H07-09": candidate("future_140_100 / future_lane", "ME084 / RI100",
                        "只对已测绘并合法开放的地籍管理一枚标牌；地址须来自编译地图。"),
    "H08-01": example("crop_24 出水作业脚", "RI162 / ME015",
                      "现有水工可处理真实田间排水；旧进水筛替换为已登记的出口。"),
    "H08-04": candidate("真实积水的田畦外侧", "ME015 / Task::Drain",
                        "可达田排水可接；要实现新浅槽需额外路径和恢复语义。"),
    "H08-09": candidate("home_well→cat_water", "ME002",
                        "原生取水与猫碗实量；待登记独立水阀状态和限流总量。"),
    "H09-01": candidate("orchard_rest / commons_garden", "RI084 / Q69",
                        "实际花期/花位可查。写生要把有限画稿存到现有展位，不能只记观察。"),
    "H09-06": candidate("home_fishing 外侧安全点", "RI082 / RI085 / 画板实体",
                        "可走到水边观察并携真实画板；需要至多一张持久倒影画稿。"),
    "H09-10": candidate("commons_hall.door/明确展位", "RI086 / Q69",
                        "同一展位可以替换有所有权的有限画稿，待明确交回和最大三幅。"),
    "H10-01": example("stall_seed.door/home_trade", "RI019 / pack_counter / counter_goods",
                      "真实种包从本人仓打包入柜可被盘点；没有展示全部种匣格。"),
    "H10-02": example("stall_seed.door/home_trade", "ME049 / ME051 / step_trip",
                      "实际双方以余菜换种。已有统一种类库存，不能宣称按旧命名季种袋变更。"),
    "H10-03": candidate("stall_seed.door", "RI014/RI017 / 真交易",
                        "只协调真实季节订单与库存；H02 园艺职能按花作或田务选择，不造一名旧访客。"),
    "H10-09": candidate("原下单摊位或 home_trade", "RI015/RI020 / 实物退件",
                        "要把同一包留种交回，来源和当前保管人均需在场。"),
    "H10-10": example("实际旧摊柜→摊主仓", "ME056 / counter reconciliation",
                      "真实剩货回仓，已接受交易才入账；原未兑现预约仍需保留对象。"),
    "A01-02": example("cat_water 邻接进食位", "Task::CatEat",
                      "橘子亲到食碗消费一份真实餐食；单独嗅闻帧可用同姿态，不是库存事件。"),
    "A01-08": candidate("已可达的原猫休息点", "Orange Task::Rest / cat_rest",
                        "给实际吃过的一餐设置短时间午休候选，保持饮水与紧急需求优先。"),
    "A01-10": candidate("cat_rest 家边床位", "Orange Task::Sleep",
                        "现有室外猫床可复用；原句的室内居处没有落实，需有实际可达入口。"),
    "A01-13": example("真实湿软地/浅雪路线", "Tracks / record_footstep",
                      "橘子真实跨格留爪印；没有永久地盘或领地判定。"),
    "A01-35": candidate("已登记家田两畦之间的通道", "真实猫步行和作物避让",
                        "只可在布置生成器和运行地图都确认的通道绕行；不得穿已耕床。"),
    "A01-37": candidate("原窄足点的安全肩位", "家庭已有限互让和步行预约",
                        "橘子与真人需在同次真实冲突内恢复双方目标，不能只把 Bond 加值。"),
    "A01-49": example("真实浅雪可走边缘", "Tracks / snow footprint",
                      "脚落在已判定浅雪或软地后留痕；没有动态滑落或地质破坏。"),
    "A01-50": candidate("已开放真实屋檐落脚", "Orange 路线 / ME078 特定避雨思路",
                        "需占用檐下站、保留旧目标并在雨停后恢复。"),
    "A14-01": example("commons_garden→果园实际产果", "ME075 / RI066 有界蜂访计数",
                      "结果进入真实果量计算；仅限已登记花源与果园，原来任选单花的效果未接。"),
}

REFERENCES.update({
    "S01": candidate("主桥候位与窄庭院合法路肩", "ME038/ME063 / living_world.rs::waiting_sidestep",
                     "家中两人已会车且恢复原目标；跨居民或物种的通用互让仍需按各自路权绑定。"),
    "S02": candidate("实际已停靠且仍可站的空位", "LivingWorld::actor_destination / Walker 的现有抵达状态",
                     "至多保存每实体一个成功落脚点；需要短停、占位复核和取消回原目标。"),
    "S03": candidate("原双方约会的相邻站位", "social_story.rs 实际预约与相邻来客交接",
                     "先做能报名的人类配对；两人恢复各自原任务才能记录结伴一次。"),
    "S04": candidate("已有檐点或 Q50 真实避雨棚", "Q50 / 居民雨天派工",
                     "现实降雨增强、无空位和恢复原目标须分别提交。"),
    "S05": Reference(NEEDS, "真实可落枝或栏杆", "ME078/ME082 专项落脚",
                     "目前只有专项避让，缺两个空中角色的同一落点权、飞行分离与复飞。"),
    "S06": Reference(NEEDS, "未来可测局部照明图", "road_story.rs Q57",
                     "只有夜灯绘制和工程位置，没有可用于寻路的暗边与暴露值。"),
    "S07": Reference(NEEDS, "未来显式物种缺口", "MapView 公用步行碰撞",
                     "未定义小体型专用连通边或落叶状态，普通碰撞仍有约束。"),
    "S08": example("实际已走过的湿软土或浅雪格", "footprints.rs / record_footstep",
                   "家庭两人及橘子有短期分体型脚印；不计其他野生对象，也不沿用旧时效变体。"),
    "S09": candidate("实际工位和相应退料来源", "Q59 清场 / ServiceLedger 退回 / ME056 收摊",
                     "可绑定已有实体散料或余料清场；没有真实料就不触发。"),
    "S10": Reference(NEEDS, "植株自己的当前格", "景物画面季节性素材",
                     "缺有界风向、风力和受力姿态。"),
    "S11": Reference(NEEDS, "真实叶面与目标土格", "living_world.rs 雨后田畦补水",
                     "现有自然雨直接影响田地；缺蓄水叶片和显式水量转移。"),
    "S12": Reference(NEEDS, "当前站位和实际亮度事件", "village_render.rs 昼夜颜色",
                     "没有亮度跨阈值的感知、最近状态与转头或停步结果。"),
    "S13": candidate("真实新设施的安全查验脚", "RI028/ME041 的命名验收",
                     "增加每实体有限最近对象、版本与占位检查，同一设施不会每天首见。"),
    "S14": candidate("到场已空的实际资源与备用点", "farm_task/market 柜台库存复核",
                     "备选已有库存约束；还需实际先到场、等待并记录换目标。"),
    "S15": Reference(NEEDS, "有权威风场的飞行路径", "野生动物基本飞行外观",
                     "缺风向、空中速度、落点和顺风产生的抵达时间变化。"),
    "S16": Reference(NEEDS, "未来动态边坡格", "MapView 动态导航",
                     "需松边对象、实际塌落、侧撤和返修的真实碰撞周期。"),
    "S17": candidate("已有活动场驻足外围", "来客容量、正常预约和释放",
                     "只做一个有限活动围观圈，保存原目的地并按观察时限散场。"),
    "S18": candidate("已修好设施的真实验收脚", "ME041 / Q50 / Q56 的付料试验",
                     "按设施类型记录真实试用与待修分支，限定已接入的设施。"),
    "S19": Reference(NEEDS, "对方最后一次见到的真实站位", "Q36/Q71 有身份记录",
                     "长期相识需另接近期视野位置、有效时限和找不到时恢复。"),
    "S20": candidate("实际落枝格及两端合法绕行", "1.1.4 自然倒木 / MapNav",
                     "复用真实有来源倒木，加受控临时阻挡和安全恢复。"),
    "S21": Reference(NEEDS, "具有可转叶片的独立植物", "季节性树冠显示",
                     "没有局部日照方向累积或新叶批次朝向。"),
    "S22": Reference(NEEDS, "已知硬物落点的近邻", "当前无通用声音事件",
                     "需事件半径、参与者排除、暂停和恢复合法路线。"),
    "S23": candidate("有限可跨干沿的实际水洼", "ME015 田洪涝 / 恶劣地面导航",
                     "已有田间洪水证据；需绑定实际浅水阻断、边沿路线和退水恢复。"),
    "S24": candidate("已有座位组或合法候行区", "home_table / 柜台预约 / ME063",
                     "先接一处真实座位组，记录新来者入位和全部散场后的释放。"),
})


@dataclass(frozen=True)
class Rule:
    id: str
    gate: str
    target: str
    motion: str
    prop: str
    partner: str
    effect: str

    @property
    def lead(self):
        return self.id[:3] if not self.id.startswith("S") else "shared"


@dataclass(frozen=True)
class Intent:
    title: str
    trigger: str
    action: str
    result: str


@dataclass(frozen=True)
class Stage:
    id: str
    base: str
    shift: int
    weather: int
    season: int
    place: int
    mode: int
    partner: str
    label: str


def data_lines(filename, skip="@#"):
    return [line.split("\t") for line in (DESIGN / filename).read_text(encoding="utf-8").splitlines()
            if line.strip() and line[0] not in skip]


def checked_index(rows, label):
    result = {}
    for row in rows:
        if row[0] in result:
            raise ValueError(f"repeated {label} {row[0]}")
        result[row[0]] = row[1:]
    return result


def authored_intent():
    all_intents = {}
    for file in ("character-humans.tsv", "character-animals-a.tsv", "character-animals-b.tsv"):
        for code, _, _, _, *fields in data_lines(file):
            all_intents[code] = Intent(*fields)
    for code, _, *fields in data_lines("character-shared.tsv"):
        all_intents[code] = Intent(*fields)
    for line in (DESIGN / "character-residents.txt").read_text(encoding="utf-8").splitlines():
        if line.startswith("@"):
            prefix, i = line[1:], 0
        elif line.strip():
            i += 1
            all_intents[f"{prefix}-{i:02d}"] = Intent(*(field.strip() for field in line.split("|")))
    return all_intents


def role_profiles():
    roles, effects = {}, {}
    for category, code, *fields in data_lines("cast-scene-profiles.tsv"):
        if category == "R":
            total, *context = fields
            roles[code] = int(total), [set(values.split(",")) for values in context]
        elif category == "E":
            effects[code] = tuple(zip(fields[::2], fields[1::2]))
        else:
            raise ValueError(f"unknown profile entry {category}")
    return roles, effects


NATIVE_ANIMAL_ORDER = (
    "A02", "A03", "A05", "A06", "A07", "A08", "A18", "A19", "A04", "A09",
    "A10", "A15", "A16", "A20", "A11", "A12", "A13", "A14", "A17",
)
# Historical time bands, used to interpret the stored scene bits.
HISTORICAL_SHIFT_MINUTES = (range(300, 450), range(450, 1080), range(1080, 1200),
                            (*range(0, 300), *range(1200, 1440)))


@dataclass(frozen=True)
class Wildlife:
    start: int
    end: int
    weather_class: int
    phase: str

    def minutes(self, shift):
        if self.start < self.end:
            return {t for t in HISTORICAL_SHIFT_MINUTES[shift] if self.start <= t < self.end}
        return {t for t in HISTORICAL_SHIFT_MINUTES[shift] if t >= self.start or t < self.end}

    def permits(self, weather, season):
        match self.weather_class:
            case 0:
                return True
            case 1:
                return weather in {"clear", "clouds"} and season != "winter"
            case 2:
                return weather == "rain" and season != "winter"
            case 3:
                return weather == "clear" and season == "summer"
            case 4:
                return weather != "snow"
            case _:
                raise ValueError(f"unrecognized native wildlife filter {self.weather_class}")


def native_wildlife():
    text = (ROOT / "src/residents.rs").read_text(encoding="utf-8")
    authored = text.split("const ANIMALS:", 1)[1].split("fn keeps_existing_terrain", 1)[0]
    found = re.findall(r'Animal \{[^}]*?"wild_(\w+)"[^}]*?active_from: (\d+),\s+'
                       r"active_to: (\d+),\s+period: [\d_]+,\s+weather: (\d),\s+mature: (true|false)",
                       authored)
    if len(found) != len(NATIVE_ANIMAL_ORDER):
        raise ValueError("native wildlife catalogue changed: review role and movement mapping")
    result = {}
    for role, (first_frame, start, end, weather, mature) in zip(NATIVE_ANIMAL_ORDER, found, strict=True):
        if not first_frame.startswith(f"{ROLES[role]}_"):
            raise ValueError(f"native wildlife order changed for {role}: {first_frame}")
        phase = "day_1" if mature == "false" else "cooperative_farm" if role == "A14" else "resident_arrival"
        result[role] = Wildlife(int(start), int(end), int(weather), phase)
    return result


def role_scope(lead, partner, species):
    if lead == "shared":
        return "按旧能力筛选；正式世界仅已具真实动作和站位的角色"
    roles = [lead] if partner == "-" else [lead, partner]
    result = []
    for role in roles:
        name = ROLES[role] or "缺同职责身份"
        if role in species:
            animal = species[role]
            name += f"（野生巡游 {animal.start}–{animal.end} 分，{animal.phase}）"
        result.append(f"{role}={name}")
    return "；".join(result)


def exact_context(rule, variant, partner, species):
    # The authored prose and stored executable condition disagree here.
    if rule.id == "H03-04":
        return "瓦工文字要求无降雨，旧数据 gate=rain；须先裁定原始条件。"
    if variant is None:
        return ""
    weather, season = CLIMATES[1][variant.weather], CLIMATES[2][variant.season]
    if weather == "rain" and season == "winter":
        return "当前世界冬季降水为雪；旧雨天冬季组合不会自然出现。"
    wildlife = set((rule.lead, partner)) & species.keys()
    possible = set(HISTORICAL_SHIFT_MINUTES[variant.shift])
    for role in sorted(wildlife):
        animal = species[role]
        if not animal.permits(weather, season):
            return f"{role} 当前巡游的天气或季节与此旧场景不相容。"
        possible &= animal.minutes(variant.shift)
        if not possible:
            return f"{role} 当前巡游时段与本场景或同伴的时间窗无交集。"
    return ""


def incidental_match(rule):
    effect = EFFECTS[rule.effect]
    site = SITES[rule.target]
    if rule.lead in NATIVE_ANIMAL_ORDER:
        return Reference(NEEDS, site, effect.evidence,
                         f"本物种有巡游外观；缺独立作业、记录或实物结果。{effect.limitation}",
                         "物种外观已存在；原动作和对应实际物件缺失或待核")
    if rule.effect in {"Nest", "Forecast"} or rule.target == "sky":
        return Reference(NEEDS, site, effect.evidence, effect.limitation,
                         "动作未接；先实现对象和真实结果，再决定复用姿态")
    return Reference(ADAPT, site, effect.evidence, effect.limitation,
                     "可复用身份和基础姿态；原物件、目标细节或独有动作待核")


def audit_row(rule, variant, intent, pairs, effects, species):
    partner = rule.partner if variant is None or variant.partner == "-" else variant.partner
    if variant is None:
        kind = "共享基础" if rule.lead == "shared" else "角色基础"
    else:
        kind = "共享变体" if rule.lead == "shared" else "配对变体" if partner != "-" else "单角色变体"
    mapping = REFERENCES.get(rule.id) or incidental_match(rule)
    context = exact_context(rule, variant, partner, species)
    absent = [actor for actor in (rule.lead, partner) if actor in PEOPLE and not PEOPLE[actor]]
    if absent:
        status, reason = ACTOR, f"{'/'.join(absent)} 当前无同职责身份。"
    elif context:
        status, reason = CONTEXT, context
    elif variant is not None and mapping.status in {UNBOUND, READY}:
        status, reason = MODE, "基础有候选；旧位置选择、效果强度、时效与配对尚未绑定。"
    else:
        status, reason = mapping.status, ""
    if status == ACTOR:
        visual = "缺同职角色和对应姿态；只能复用候选基础美术"
    elif mapping.status == NEEDS:
        visual = "有一般角色或场景外观；缺合格的真实动作、对应物件或位置表现"
    else:
        visual = mapping.visual
    if variant is None:
        shift, weather, season, place = ("—",) * 4
        profile_name, profile_result = "基础效果", intent.result
    else:
        shift, weather, season = (
            labels[index] for labels, index in zip(CLIMATES, (variant.shift, variant.weather, variant.season), strict=True)
        )
        place = PLACEMENTS[variant.place]
        profile_name, profile_result = effects[rule.effect][variant.mode - 1]
    return {
        "旧ID": rule.id if variant is None else variant.id,
        "基础ID": rule.id, "类型": kind, "旧主体": rule.lead, "旧伴侣": partner,
        "旧意图": intent.title if variant is None else variant.label,
        "旧触发码": rule.gate, "旧目标类": rule.target, "旧姿态": rule.motion, "旧物件词": rule.prop,
        "旧效果": rule.effect, "旧时段": shift, "旧天气": weather, "旧季节": season, "旧位置": place,
        "旧变化方式": profile_name, "旧变化结果": profile_result,
        "原作者条件": intent.trigger, "原作者动作": intent.action, "原作者结果": intent.result,
        "配对设计说明": pairs.get((rule.lead, partner), ("", ""))[1],
        "正式可用身份": role_scope(rule.lead, partner, species),
        "正式候选场地": mapping.site, "正式机制证据": mapping.evidence,
        "核对方式": "逐基础意图配置候选" if rule.id in REFERENCES else "继承基础原文并比对机制族",
        "映射判定": status, "映射依据与边界": (reason + mapping.note).replace("；", "。"),
        "画面状况": visual.replace("；", "。"),
        "正式旧ID绑定": DISCLOSURE,
    }


def validate_variants(variants, rules, roles, pairs):
    for variant in variants:
        rule = rules[variant.base]
        values = (variant.shift, variant.weather, variant.season)
        if not all(0 <= value < len(labels) for labels, value in zip(CLIMATES, values, strict=True)):
            raise ValueError(f"invalid variant climate {variant.id}")
        if not 0 <= variant.place < len(PLACEMENTS) or not 1 <= variant.mode <= 3:
            raise ValueError(f"invalid variant presentation {variant.id}")
        if variant.partner != "-":
            allowed, _ = pairs[rule.lead, variant.partner]
            if rule.effect not in allowed:
                raise ValueError(f"incompatible pair effect {variant.id}")
        for actor in (rule.lead, variant.partner):
            if actor in roles and not all(labels[value] in supported for labels, value, supported in
                                          zip(CLIMATES, values, roles[actor][1], strict=True)):
                raise ValueError(f"legacy profile and generated scene disagree {variant.id}")


def build_table():
    roles, effects = role_profiles()
    pair_rows = data_lines("cast-pairs.tsv")
    pairs = {(lead, partner): (kinds.split(","), detail) for lead, partner, kinds, detail in pair_rows}
    intent = authored_intent()
    rules = {code: Rule(code, *fields) for code, fields in checked_index(data_lines("cast-rules.tsv"), "base").items()}
    variants = [Stage(code, base, *map(int, key), partner, title)
                for code, (base, *key, partner, title) in checked_index(data_lines("cast-scenes.tsv"), "variant").items()]
    if set(rules) != set(intent) or set(roles) != set(ROLES) or set(effects) != set(EFFECTS):
        raise ValueError("historical base or profile inventories changed")
    if len(pairs) != 45 or len(pair_rows) != len(pairs) or set(REFERENCES) - rules.keys():
        raise ValueError("historical pair or manually reviewed IDs changed")
    if any(len(modes) != 3 for modes in effects.values()):
        raise ValueError("a legacy effect must have exactly three variant modes")
    village = json.loads(BLUEPRINT.read_text(encoding="utf-8"))
    map_ids = {row["id"] for table in ("pois", "roads", "buildings", "lots", "crops") for row in village[table]}
    if not {"home_xiaohe.door", "cat_rest", "home_trade", "crop_06", "orchard_tools",
            "pantry_loading", "south_apiary", "future_140_100", "commons_hall.door"} <= map_ids:
        raise ValueError("a reviewed authored POI moved or disappeared")
    actual_residents = {row["id"] for row in village["residents"]}
    for person in (PEOPLE[role].split("（")[0].split("/") for role in PEOPLE if role != "H01" and PEOPLE[role]):
        if not set(person) <= actual_residents:
            raise ValueError(f"review native role identity change {person}")
    wildlife = native_wildlife()
    validate_variants(variants, rules, roles, pairs)
    rows = [audit_row(rule, None, intent[rule.id], pairs, effects, wildlife) for rule in rules.values()]
    rows.extend(audit_row(rules[scene.base], scene, intent[scene.base], pairs, effects, wildlife) for scene in variants)
    kind_counts = Counter(row["类型"] for row in rows)
    if len({row["旧ID"] for row in rows}) != 5000 or kind_counts != {
        "角色基础": 400, "共享基础": 24, "单角色变体": 2600, "共享变体": 1076, "配对变体": 900
    }:
        raise ValueError(f"catalogue count changed: {kind_counts}")
    if BINDINGS.exists():
        with BINDINGS.open(encoding="utf-8", newline="") as source:
            index = {row["旧ID"]: row for row in rows}
            seen = set()
            for binding in csv.DictReader(source, delimiter="\t"):
                code = binding["old_id"]
                if code in seen or code not in index:
                    raise ValueError(f"invalid or duplicate registered legacy ID {code}")
                expected_kind = "base" if code in rules else "variant"
                expected_base = code.split("~", 1)[0].split("+", 1)[0]
                if (binding["base_id"], binding["kind"]) != (expected_base, expected_kind):
                    raise ValueError(f"registered legacy ID has the wrong origin: {code}")
                seen.add(code)
                row = index[code]
                row["正式旧ID绑定"] = "已登记基础执行入口" if expected_kind == "base" else "已登记条件匹配场景"
                row["正式机制证据"] += f"。正式登记：{binding['implementation']}"
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="compare against the checked-in audit without writing")
    args = parser.parse_args()
    rows = build_table()
    stream = io.StringIO()
    writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    contents = stream.getvalue().encode("utf-8")
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_bytes() != contents:
            parser.exit(1, f"{OUTPUT.name} is stale. Re-run audit_cast_rules.py to refresh it.\n")
    else:
        OUTPUT.write_bytes(contents)
    for key, part in (("all", rows), ("base", rows[:424]), ("variants", rows[424:])):
        result = Counter(row["映射判定"] for row in part)
        print(f"{key}: {len(part)}  {dict(result)}")
    print(f"registered: {dict(Counter(row['正式旧ID绑定'] for row in rows))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
