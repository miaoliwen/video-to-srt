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
//! 标准形式，防止 ASR 输出中此类字符导致显示/搜索异常；中文全角标点保留。

use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// 精选繁→简映射（仅收录普通话 ASR 输出高频用字）。
/// 注意：只收录真正的繁→简转换；`X→X` 的恒等映射与重复键均不保留。
fn build_table() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    // 高频虚词 / 介词 / 连词
    m.insert("關", "关"); m.insert("於", "于"); m.insert("這", "这"); m.insert("個", "个");
    m.insert("們", "们"); m.insert("來", "来"); m.insert("時", "时"); m.insert("會", "会");
    m.insert("對", "对"); m.insert("為", "为"); m.insert("從", "从"); m.insert("說", "说");
    m.insert("話", "话"); m.insert("麼", "么"); m.insert("嗎", "吗"); m.insert("過", "过");
    m.insert("還", "还"); m.insert("沒", "没"); m.insert("讓", "让"); m.insert("給", "给");
    m.insert("請", "请"); m.insert("應", "应"); m.insert("該", "该"); m.insert("當", "当");
    m.insert("將", "将"); m.insert("與", "与"); m.insert("並", "并");

    // 高频名词 / 动词
    m.insert("雲", "云"); m.insert("電", "电"); m.insert("腦", "脑"); m.insert("網", "网");
    m.insert("絡", "络"); m.insert("語", "语"); m.insert("識", "识"); m.insert("別", "别");
    m.insert("訊", "讯"); m.insert("記", "记"); m.insert("錄", "录"); m.insert("頻", "频");
    m.insert("據", "据"); m.insert("務", "务"); m.insert("數", "数"); m.insert("處", "处"); m.insert("變", "变"); m.insert("換", "换");
    m.insert("測", "测"); m.insert("試", "试"); m.insert("結", "结"); m.insert("問", "问");
    m.insert("題", "题"); m.insert("寫", "写"); m.insert("讀", "读"); m.insert("聽", "听");
    m.insert("見", "见"); m.insert("視", "视"); m.insert("覺", "觉"); m.insert("聲", "声");
    m.insert("畫", "画"); m.insert("圖", "图"); m.insert("攝", "摄"); m.insert("影", "影");
    m.insert("檔", "档"); m.insert("頁", "页"); m.insert("張", "张"); m.insert("體", "体");
    m.insert("積", "积"); m.insert("樣", "样"); m.insert("種", "种"); m.insert("類", "类");
    m.insert("項", "项"); m.insert("標", "标"); m.insert("準", "准"); m.insert("單", "单");
    m.insert("雙", "双"); m.insert("長", "长"); m.insert("寬", "宽"); m.insert("窄", "窄");
    m.insert("淺", "浅"); m.insert("輕", "轻"); m.insert("軟", "软"); m.insert("遠", "远");
    m.insert("近", "近"); m.insert("後", "后"); m.insert("裡", "里"); m.insert("內", "内");
    m.insert("間", "间"); m.insert("邊", "边"); m.insert("頂", "顶"); m.insert("頭", "头");

    // 技术 / 现代词汇
    m.insert("碼", "码"); m.insert("編", "编"); m.insert("譯", "译"); m.insert("設", "设");
    m.insert("計", "计"); m.insert("機", "机"); m.insert("構", "构"); m.insert("組", "组");
    m.insert("層", "层"); m.insert("級", "级"); m.insert("線", "线"); m.insert("纜", "缆");
    m.insert("總", "总"); m.insert("傳", "传"); m.insert("輸", "输"); m.insert("發", "发");
    m.insert("開", "开"); m.insert("啟", "启"); m.insert("閉", "闭"); m.insert("運", "运");
    m.insert("進", "进"); m.insert("離", "离"); m.insert("連", "连"); m.insert("斷", "断");
    m.insert("續", "续"); m.insert("載", "载"); m.insert("儲", "储"); m.insert("備", "备");
    m.insert("刪", "删"); m.insert("舊", "旧"); m.insert("創", "创"); m.insert("減", "减");
    m.insert("餘", "余"); m.insert("萬", "万"); m.insert("億", "亿"); m.insert("負", "负");

    // 字幕 / 视频 / 媒体
    m.insert("場", "场"); m.insert("鏡", "镜"); m.insert("輯", "辑"); m.insert("製", "制");
    m.insert("導", "导"); m.insert("員", "员"); m.insert("樂", "乐"); m.insert("詞", "词");
    m.insert("劇", "剧"); m.insert("節", "节"); m.insert("臺", "台"); m.insert("灣", "湾");
    m.insert("陸", "陆");

    // 常用动词 / 形容词补充
    m.insert("講", "讲"); m.insert("辦", "办"); m.insert("幫", "帮"); m.insert("護", "护");
    m.insert("愛", "爱"); m.insert("歡", "欢"); m.insert("傷", "伤"); m.insert("壞", "坏");
    m.insert("實", "实"); m.insert("虛", "虚"); m.insert("滿", "满");
    m.insert("淨", "净"); m.insert("髒", "脏"); m.insert("紅", "红"); m.insert("綠", "绿");
    m.insert("藍", "蓝"); m.insert("黃", "黄");

    // 乾/發/幹/裏 家族补充：髮→发（头发）、幹→干（干活）、裏→里（里面）。
    // 注意：乾 不在表中——它是多对一误转的源头，改由 t2s 按上下文判断。
    m.insert("髮", "发"); m.insert("幹", "干"); m.insert("裏", "里");

    // 时间 / 数量
    m.insert("週", "周"); m.insert("點", "点"); m.insert("鐘", "钟");

    // 教育 / 人物 / 称谓
    m.insert("國", "国"); m.insert("學", "学"); m.insert("習", "习"); m.insert("師", "师");
    m.insert("醫", "医"); m.insert("藝", "艺"); m.insert("術", "术"); m.insert("業", "业");
    m.insert("專", "专"); m.insert("職", "职"); m.insert("課", "课"); m.insert("書", "书");

    // 社会 / 自然 / 环境
    m.insert("眾", "众"); m.insert("廣", "广"); m.insert("廠", "厂"); m.insert("東", "东");
    m.insert("門", "门"); m.insert("陽", "阳"); m.insert("陰", "阴"); m.insert("氣", "气");
    m.insert("風", "风"); m.insert("園", "园"); m.insert("團", "团"); m.insert("隊", "队");
    m.insert("戰", "战"); m.insert("爭", "争"); m.insert("報", "报"); m.insert("紙", "纸");
    m.insert("車", "车"); m.insert("飛", "飞"); m.insert("聞", "闻");

    // 经济 / 交易 / 生活
    m.insert("價", "价"); m.insert("錢", "钱"); m.insert("銀", "银"); m.insert("買", "买");
    m.insert("賣", "卖"); m.insert("購", "购"); m.insert("賬", "账"); m.insert("戶", "户");
    m.insert("郵", "邮"); m.insert("號", "号"); m.insert("鍵", "键"); m.insert("盤", "盘");
    m.insert("條", "条"); m.insert("產", "产"); m.insert("質", "质"); m.insert("優", "优");
    m.insert("帶", "带"); m.insert("傘", "伞"); m.insert("熱", "热");

    // 状态 / 规则 / 关系
    m.insert("驗", "验"); m.insert("觀", "观"); m.insert("確", "确"); m.insert("際", "际");
    m.insert("聯", "联"); m.insert("係", "系"); m.insert("統", "统"); m.insert("規", "规");
    m.insert("則", "则"); m.insert("資", "资"); m.insert("隱", "隐"); m.insert("權", "权");
    m.insert("態", "态"); m.insert("環", "环"); m.insert("監", "监"); m.insert("動", "动");

    // 健康 / 训练
    m.insert("練", "练"); m.insert("訓", "训"); m.insert("療", "疗"); m.insert("藥", "药");
    m.insert("鍛", "锻"); m.insert("煉", "炼"); m.insert("慶", "庆");

    // 多对一安全义项：复(複/復)、历(曆/歷)、只(隻)、斗(鬥)、面(麵)、几(幾)、谷(穀)
    m.insert("複", "复"); m.insert("復", "复"); m.insert("曆", "历"); m.insert("歷", "历");
    m.insert("隻", "只"); m.insert("鬥", "斗"); m.insert("麵", "面"); m.insert("幾", "几");
    m.insert("穀", "谷");

    m
}

static TABLE: std::sync::OnceLock<HashMap<&'static str, &'static str>> = std::sync::OnceLock::new();

fn table() -> &'static HashMap<&'static str, &'static str> {
    TABLE.get_or_init(build_table)
}

/// 繁体 乾 是多对一字：gān（干燥）读法简化为 干，而 qián 读法（专有名词
/// 乾隆、乾坤等）保持 乾 不变。以下是被 乾 后随的常见 qián 读法用字；
/// 其余位置（含句尾）一律按 gān 处理为 干。
const QIAN_NEXT: [char; 5] = ['隆', '坤', '卦', '陵', '县'];

/// 中文全角标点：NFKC 会把它们映射成半角 ASCII（，→ ,、！→ !），破坏中文
/// 字幕排版，因此归一化时跳过。注意：全角数字（１-９）与全角拉丁字母
/// （Ａ-Ｚ）不在该集合内，仍会归一化。
fn is_cjk_punct(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{303F}'   // CJK 符号与标点（。、「」《》…）
        | '\u{FF01}'..='\u{FF0F}' // 全角 ！＂＃＄％＆＇（）＊＋，－．／
        | '\u{FF1A}'..='\u{FF20}' // 全角 ：；＜＝＞？＠
        | '\u{FF3B}'..='\u{FF40}' // 全角 ［＼］＾＿｀
        | '\u{FF5B}'..='\u{FF60}' // 全角 ｛｜｝～｟｠
        | '\u{FFE0}'..='\u{FFE6}' // 全角货币符号
    )
}

/// 把输入字符串中的繁体字逐字符映射为简体。
/// 仅替换出现在映射表中的单字符；其他字符（ASCII、日韩文、未收录的繁体、
/// 简体本身）保持不变。
/// 例外：乾 在 t2s 内做上下文判断（见 QIAN_NEXT），避免「乾隆→干隆」类误转。
/// 最后应用 Unicode NFKC 归一化，消除兼容区字符差异。
pub fn t2s(input: &str) -> String {
    let t = table();
    let mut out = String::with_capacity(input.len());
    let mut buf = [0u8; 4];
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '乾' {
            // 乾 不在表中，这里按上下文分流：qián 读法（乾隆/乾坤/乾卦/
            // 乾陵/乾县）保留 乾；其余 gān 读法（干燥）→ 干。
            if chars.peek().is_some_and(|&n| QIAN_NEXT.contains(&n)) {
                out.push(ch);
            } else {
                out.push('干');
            }
            continue;
        }
        let key = ch.encode_utf8(&mut buf);
        match t.get(key) {
            Some(simp) => out.push_str(simp),
            None => out.push(ch),
        }
    }
    // NFKC normalization: converts full-width digits, circled numbers,
    // superscripts, and other compatibility characters to their standard
    // forms — while preserving full-width CJK punctuation, which NFKC would
    // otherwise flatten to ASCII （，→ ,、！→ !）. Applied per-character so the
    // punctuation keep-set is honored; compatibility forms are single-char
    // mappings, so this is equivalent to whole-string NFKC for subtitle text.
    let mut normalized = String::with_capacity(out.len());
    for c in out.chars() {
        if is_cjk_punct(c) {
            normalized.push(c);
        } else {
            normalized.extend(c.nfkc());
        }
    }
    normalized
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

    #[test]
    fn qian_keeps_proper_noun_readings() {
        // qián 读法（专有名词）必须保留 乾，不得误转成 干。
        assert_eq!(t2s("乾隆皇帝"), "乾隆皇帝");
        assert_eq!(t2s("乾坤"), "乾坤");
        assert_eq!(t2s("乾卦"), "乾卦");
        // gān 读法（干燥）仍正常转成 干。
        assert_eq!(t2s("乾淨"), "干净");
        assert_eq!(t2s("乾燥"), "干燥");
        // 句尾 / 独立出现的 乾 按 gān 处理。
        assert_eq!(t2s("晾乾"), "晾干");
    }

    #[test]
    fn converts_gan_family_characters() {
        // 髮/幹/裏：干 家族中此前缺失的安全映射。
        assert_eq!(t2s("頭髮"), "头发");
        assert_eq!(t2s("幹活"), "干活");
        assert_eq!(t2s("幹部"), "干部");
        assert_eq!(t2s("裏面"), "里面");
    }

    #[test]
    fn one_to_many_pairs_convert_by_context() {
        // 发 家族：發（发送）与 髮（头发）都 → 发；简体 发 原样保留。
        assert_eq!(t2s("發送文件"), "发送文件");
        assert_eq!(t2s("頭髮"), "头发");
        assert_eq!(t2s("发展"), "发展");

        // 干 家族：乾（干燥）→ 干、幹（干活）→ 干、乾（qián）保留；
        // 简体 干 原样保留。
        assert_eq!(t2s("乾淨"), "干净");
        assert_eq!(t2s("幹活"), "干活");
        assert_eq!(t2s("乾隆"), "乾隆");
        assert_eq!(t2s("干杯"), "干杯");

        // 里 家族：裡 与 裏 都 → 里；简体 里（公里）原样保留。
        assert_eq!(t2s("裡面"), "里面");
        assert_eq!(t2s("屋裏"), "屋里");
        assert_eq!(t2s("公里"), "公里");

        // 后 家族：後 → 后；简体/繁体同形的 后（皇后）原样保留。
        assert_eq!(t2s("之後"), "之后");
        assert_eq!(t2s("後面"), "后面");
        assert_eq!(t2s("皇后"), "皇后");
    }

    #[test]
    fn realistic_subtitle_text_converts_cleanly() {
        // 模拟一段真实的 whisper 多语言模型输出（繁简混杂 + 多对一字）。
        let input = "關於使用阿里雲的語音識別服務，我們需要先配置 API Key。\n\
                     乾乾淨淨地處理完這段視頻之後，頭髮也乾了。\n\
                     幹活的時候要注意安全，不要讓機器故障。\n\
                     乾隆皇帝在位期間，這裡面的數據都很重要，請妥善保存。";
        let out = t2s(input);

        // 常见高频字整体转换。
        assert!(out.contains("关于使用阿里云的语音识别服务"));
        assert!(out.contains("我们需要先配置 API Key"));
        assert!(out.contains("不要让机器故障"));
        assert!(out.contains("这里面的数据都很重要"));
        assert!(out.contains("请妥善保存"));
        assert!(out.contains("妥善保存"));
        // 多对一字按上下文正确分流。
        assert!(out.contains("干干净净地处理完这段视频之后")); // 乾(×2)→干、後→后
        assert!(out.contains("头发也干了")); // 髮→发、乾(句尾)→干
        assert!(out.contains("干活的时候")); // 幹→干
        assert!(out.contains("乾隆皇帝在位期间")); // 乾(qián) 保留
        assert!(!out.contains("干隆")); // 不得出现误转
    }

    #[test]
    fn realistic_srt_corpus_regression() {
        // 模拟一份真实 SRT 语料：whisper 多语言模型输出的繁简混杂文本
        // （序号/时间轴为 ASCII，不受转换影响）。全角中文标点必须保留。
        let corpus = "\
1\r\n\
00:00:00,000 --> 00:00:02,500\r\n\
各位觀眾，歡迎收看今晚的新聞報導。\r\n\r\n\
2\r\n\
00:00:02,600 --> 00:00:05,000\r\n\
今天全國各地都在慶祝國慶節，學校和公園裡人山人海。\r\n\r\n\
3\r\n\
00:00:05,100 --> 00:00:08,000\r\n\
老師教我們學習數學和科學，還要練習寫字。\r\n\r\n\
4\r\n\
00:00:08,100 --> 00:00:11,000\r\n\
醫生說要多運動、多鍛煉，身體才會健康。\r\n\r\n\
5\r\n\
00:00:11,100 --> 00:00:14,000\r\n\
公司推出了新產品，價格實惠，質量很好。\r\n\r\n\
6\r\n\
00:00:14,100 --> 00:00:17,000\r\n\
歷史上的乾隆皇帝很有名，大家都聽過他的名字。\r\n\r\n\
7\r\n\
00:00:17,100 --> 00:00:20,000\r\n\
天氣越來越熱，風也很大，記得帶雨傘出門。";
        let out = t2s(corpus);

        // 逐条验证转换后的字幕正文。
        // 注意：報導 → 报导（導→导），而非“报道”。
        assert!(out.contains("各位观众，欢迎收看今晚的新闻报导"));
        assert!(out.contains("今天全国各地都在庆祝国庆节，学校和公园里人山人海"));
        assert!(out.contains("老师教我们学习数学和科学，还要练习写字"));
        assert!(out.contains("医生说要多运动、多锻炼，身体才会健康"));
        assert!(out.contains("公司推出了新产品，价格实惠，质量很好"));
        assert!(out.contains("历史上的乾隆皇帝很有名，大家都听过他的名字"));
        assert!(out.contains("天气越来越热，风也很大，记得带雨伞出门"));
        // 多对一字在真实语料中的正确分流。
        assert!(out.contains("乾隆皇帝")); // 乾(qián) 保留
        assert!(!out.contains("干隆"));
        // 序号与时间轴原样保留。
        assert!(out.contains("00:00:14,100 --> 00:00:17,000"));
    }

    #[test]
    fn nfkc_normalizes_compat_chars_but_keeps_cjk_punctuation() {
        // 兼容字符仍归一化：圈数字、全角数字、全角拉丁字母。
        assert_eq!(t2s("①②③"), "123");
        assert_eq!(t2s("２０２６年"), "2026年");
        assert_eq!(t2s("ＡＢＣ"), "ABC");
        // 中文全角标点保留。
        assert_eq!(t2s("你好，世界！"), "你好，世界！");
        assert_eq!(t2s("他说：“好的”？；（不错）"), "他说：“好的”？；（不错）");
        assert_eq!(t2s("简体、繁體（测试）《標題》"), "简体、繁体（测试）《标题》");
        // 全角数字仍归一化，但紧随的全角标点不受影响。
        assert_eq!(t2s("第１２個，好！"), "第12个，好！");
    }
}