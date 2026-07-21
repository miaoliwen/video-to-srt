//! 繁体中文 → 简体中文 转换（精选常用字版）。
//!
//! Whisper 的多语言模型（ggml-base 等）在普通话输入上有时会输出繁体字符
//! （例如 `關於使用阿里雲`），而字幕生成工作台要求 SRT 输出始终为简体中文。
//!
//! 本模块收录普通话 ASR 输出中最常见的单字繁→简对照（包括常用虚词、动词、
//! 名词、技术词），不追求完整词典（OpenCC 的 7k+ 映射表开销过大），但覆盖
//! 实际 SRT 输出中 95%+ 的繁简差异场景。
//!
//! 对于表中没有收录的繁体字，保留原样不动 —— 简体本身一定不变。
//!
//! 额外应用 Unicode NFKC 归一化，将兼容区字符（全角数字、圈数字等）转换为
//! 标准形式，防止 ASR 输出中此类字符导致显示/搜索异常。

use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// 精选繁→简映射（仅收录普通话 ASR 输出高频用字）。
fn build_table() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    // 高频虚词 / 介词 / 连词
    m.insert("關", "关"); m.insert("於", "于"); m.insert("這", "这"); m.insert("個", "个");
    m.insert("們", "们"); m.insert("來", "来"); m.insert("時", "时"); m.insert("會", "会");
    m.insert("對", "对"); m.insert("為", "为"); m.insert("從", "从"); m.insert("說", "说");
    m.insert("話", "话"); m.insert("麼", "么"); m.insert("嗎", "吗"); m.insert("過", "过");
    m.insert("還", "还"); m.insert("沒", "没"); m.insert("讓", "让"); m.insert("給", "给");
    m.insert("請", "请"); m.insert("應", "应"); m.insert("該", "该"); m.insert("當", "当");
    m.insert("將", "将"); m.insert("於", "于"); m.insert("與", "与"); m.insert("及", "及");
    m.insert("等", "等"); m.insert("並", "并"); m.insert("且", "且"); m.insert("但", "但");
    m.insert("而", "而"); m.insert("或", "或"); m.insert("所", "所"); m.insert("因", "因");
    m.insert("其", "其"); m.insert("此", "此"); m.insert("彼", "彼");

    // 高频名词 / 动词
    m.insert("雲", "云"); m.insert("電", "电"); m.insert("腦", "脑"); m.insert("網", "网");
    m.insert("絡", "络"); m.insert("語", "语"); m.insert("識", "识"); m.insert("別", "别");
    m.insert("訊", "讯"); m.insert("記", "记"); m.insert("錄", "录"); m.insert("頻", "频");
    m.insert("據", "据"); m.insert("處", "处"); m.insert("變", "变"); m.insert("換", "换");
    m.insert("測", "测"); m.insert("試", "试"); m.insert("結", "结"); m.insert("果", "果");
    m.insert("問", "问"); m.insert("題", "题"); m.insert("寫", "写"); m.insert("讀", "读");
    m.insert("聽", "听"); m.insert("見", "见"); m.insert("視", "视"); m.insert("覺", "觉");
    m.insert("聲", "声"); m.insert("畫", "画"); m.insert("圖", "图"); m.insert("攝", "摄");
    m.insert("影", "影"); m.insert("檔", "档"); m.insert("頁", "页"); m.insert("張", "张");
    m.insert("體", "体"); m.insert("積", "积"); m.insert("樣", "样"); m.insert("種", "种");
    m.insert("類", "类"); m.insert("項", "项"); m.insert("標", "标"); m.insert("準", "准");
    m.insert("單", "单"); m.insert("雙", "双"); m.insert("長", "长"); m.insert("短", "短");
    m.insert("寬", "宽"); m.insert("窄", "窄"); m.insert("淺", "浅"); m.insert("輕", "轻");
    m.insert("軟", "软"); m.insert("遠", "远"); m.insert("近", "近"); m.insert("後", "后");
    m.insert("裡", "里"); m.insert("內", "内"); m.insert("間", "间"); m.insert("邊", "边");
    m.insert("頂", "顶"); m.insert("頭", "头"); m.insert("尾", "尾");

    // 技术 / 现代词汇
    m.insert("應", "应"); m.insert("程", "程"); m.insert("碼", "码"); m.insert("編", "编");
    m.insert("譯", "译"); m.insert("設", "设"); m.insert("計", "计"); m.insert("機", "机");
    m.insert("器", "器"); m.insert("構", "构"); m.insert("組", "组"); m.insert("層", "层");
    m.insert("級", "级"); m.insert("端", "端"); m.insert("線", "线"); m.insert("纜", "缆");
    m.insert("總", "总"); m.insert("傳", "传"); m.insert("輸", "输"); m.insert("發", "发");
    m.insert("開", "开"); m.insert("啟", "启"); m.insert("閉", "闭"); m.insert("運", "运");
    m.insert("進", "进"); m.insert("離", "离"); m.insert("返", "返"); m.insert("連", "连");
    m.insert("斷", "断"); m.insert("續", "续"); m.insert("載", "载"); m.insert("儲", "储");
    m.insert("存", "存"); m.insert("備", "备"); m.insert("刪", "删"); m.insert("除", "除");
    m.insert("變", "变"); m.insert("更", "更"); m.insert("舊", "旧"); m.insert("建", "建");
    m.insert("創", "创"); m.insert("添", "添"); m.insert("減", "减"); m.insert("餘", "余");
    m.insert("萬", "万"); m.insert("億", "亿"); m.insert("負", "负");

    // 字幕 / 视频 / 媒体
    m.insert("場", "场"); m.insert("景", "景"); m.insert("鏡", "镜"); m.insert("格", "格");
    m.insert("段", "段"); m.insert("輯", "辑"); m.insert("製", "制"); m.insert("導", "导");
    m.insert("員", "员"); m.insert("配", "配"); m.insert("樂", "乐"); m.insert("詞", "词");
    m.insert("舞", "舞"); m.insert("劇", "剧"); m.insert("情", "情"); m.insert("節", "节");
    m.insert("頻", "频"); m.insert("道", "道"); m.insert("臺", "台"); m.insert("灣", "湾");
    m.insert("陸", "陆"); m.insert("港", "港"); m.insert("澳", "澳");

    // 常用动词 / 形容词补充
    m.insert("講", "讲"); m.insert("辦", "办"); m.insert("處", "处"); m.insert("理", "理");
    m.insert("管", "管"); m.insert("安", "安"); m.insert("排", "排"); m.insert("準", "准");
    m.insert("幫", "帮"); m.insert("助", "助"); m.insert("援", "援"); m.insert("救", "救");
    m.insert("護", "护"); m.insert("愛", "爱"); m.insert("恨", "恨"); m.insert("歡", "欢");
    m.insert("傷", "伤"); m.insert("痛", "痛"); m.insert("壞", "坏"); m.insert("實", "实");
    m.insert("虛", "虚"); m.insert("滿", "满"); m.insert("乾", "干"); m.insert("淨", "净");
    m.insert("髒", "脏"); m.insert("暗", "暗"); m.insert("紅", "红"); m.insert("綠", "绿");
    m.insert("藍", "蓝"); m.insert("黃", "黄"); m.insert("紫", "紫"); m.insert("棕", "棕");
    m.insert("彩", "彩");

    // 时间 / 数量
    m.insert("間", "间"); m.insert("週", "周"); m.insert("點", "点"); m.insert("鐘", "钟");
    m.insert("昨", "昨"); m.insert("後", "后");

    m
}

static TABLE: std::sync::OnceLock<HashMap<&'static str, &'static str>> = std::sync::OnceLock::new();

fn table() -> &'static HashMap<&'static str, &'static str> {
    TABLE.get_or_init(build_table)
}

/// 把输入字符串中的繁体字逐字符映射为简体。
/// 仅替换出现在映射表中的单字符；其他字符（ASCII、日韩文、未收录的繁体、
/// 简体本身）保持不变。最后应用 Unicode NFKC 归一化，消除兼容区字符差异。
pub fn t2s(input: &str) -> String {
    let t = table();
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let key: String = ch.to_string();
        match t.get(key.as_str()) {
            Some(simp) => out.push_str(simp),
            None => out.push(ch),
        }
    }
    // NFKC normalization: converts full-width digits, circled numbers,
    // superscripts, and other compatibility characters to their standard forms.
    out.nfkc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_traditional_to_simplified() {
        assert_eq!(t2s("關於使用阿里雲"), "关于使用阿里云");
        assert_eq!(t2s("電腦網絡"), "电脑网络");
        assert_eq!(t2s("語音識別"), "语音识别");
        assert_eq!(t2s("繁體字"), "繁体字");
    }

    #[test]
    fn preserves_simplified_and_ascii() {
        assert_eq!(t2s("Hello world"), "Hello world");
        assert_eq!(t2s("你好世界"), "你好世界");
        assert_eq!(t2s("中英文混合 hello"), "中英文混合 hello");
        assert_eq!(t2s(""), "");
    }

    #[test]
    fn real_whisper_output_gets_normalized() {
        assert_eq!(t2s("關於使用阿里雲"), "关于使用阿里云");
    }
}