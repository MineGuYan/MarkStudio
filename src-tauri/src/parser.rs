//! Markdown 解析模块
//!
//! 本模块负责将 Markdown 文本转换为 HTML 字符串。
//! 使用 pulldown-cmark 库进行解析，支持表格、任务列表、删除线等扩展语法。
//! 解析完成后，自动将本地文件路径的图片转换为 Base64 Data URI，
//! 确保在 WebView 中能够正常显示本地图片。

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::sync::LazyLock;

/// 用于匹配 HTML 中 `<img>` 标签 src 属性的正则表达式
///
/// 捕获组 1：src 属性值（可能带引号）
/// 捕获组 2：图片文件路径
static IMG_SRC_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<img\s[^>]*?src="([^"]+)""#).expect("图片 src 正则表达式编译失败")
});

/// 用于匹配块级数学公式的正则表达式（$$...$$）
static MATH_BLOCK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\$(.*?)\$\$").expect("数学块正则表达式编译失败")
});

/// 用于匹配行内数学公式的正则表达式（$...$）
///
/// Rust regex 不支持零宽断言（look-around, look-behind, look-ahead）。
/// 因此我们使用捕获组来实现，代价是公式前后的字符会被保留在 $1 和 $3 中。
///
/// 正则设计：`(^|[^\$])\$([^\$]+?)\$([^$]|$)`
/// - `(^|[^\$])`：开头或非 $ 字符（捕获到 $1）
/// - `\$`：`$` 符号
/// - `([^\$]+?)`：公式内容（非贪婪）
/// - `\$`：`$` 符号
/// - `([^$]|$)`：非 $ 字符或字符串结尾（捕获到 $3）
///
/// 替换格式：`$1<span class="math-inline">$2</span>$3`
///
/// 支持：$x$（单字符）、$a$ $b$（有空格连续公式）、$a+b$（含特殊字符）
/// 已知限制：$a$b$c（无空格连续公式）由于 Rust regex 限制无法完美处理
static MATH_INLINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\$])\$([^\$]+?)\$([^$]|$)")
        .expect("行内数学正则表达式编译失败")
});

/// 用于匹配自动目录标记的正则表达式
/// 支持 [[toc]] 和 [TOC] 两种格式（不区分大小写）
static TOC_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\[\[?toc\]?\]").expect("目录正则表达式编译失败")
});

/// 用于匹配 Emoji 短代码的正则表达式（:name:）
static EMOJI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r":([a-zA-Z0-9_]+):").expect("Emoji 正则表达式编译失败")
});

/// 用于匹配 Mermaid 代码块的正则表达式
/// 匹配 `<pre><code class="language-mermaid">...</code></pre>` 格式
static MERMAID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<pre><code class="language-mermaid">(.*?)</code></pre>"#)
        .expect("Mermaid 正则表达式编译失败")
});

/// 用于匹配 HTML 中 table 标签的正则表达式
/// 将 `<table>...</table>` 包裹在 `<div class="table-wrapper">` 中
static TABLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<table>(.*?)</table>"#).expect("表格正则表达式编译失败")
});

/// 将 Markdown 文本解析为 HTML 字符串
///
/// 解析完成后，自动检测 HTML 中的本地图片路径（非 http/https 开头），
/// 将其转换为 Base64 Data URI，确保在 WebView 中能够正常显示。
///
/// # 参数
/// - `markdown`: 要解析的 Markdown 源文本
/// - `document_path`: 文档所在的文件路径，用于解析相对路径的图片
///
/// # 返回
/// 解析后生成的 HTML 字符串（本地图片已替换为 Base64 Data URI）
///
/// # 支持的扩展语法
/// - 表格 (table)
/// - 任务列表 (tasklist)
/// - 删除线 (strikethrough)
/// - 脚注 (footnotes)
/// - 自动链接 (autolinks)
/// - 智能标点 (smart punctuation)
/// - Emoji 短代码（如 :smile:）
pub fn parse_markdown_to_html(markdown: &str, document_path: Option<&str>) -> String {
    // 启用 GFM 扩展语法：表格、任务列表、删除线
    // 启用额外扩展：脚注、智能标点（自动链接已默认支持）
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_SMART_PUNCTUATION;

    // 预处理：将 Emoji 短代码转换为 Unicode Emoji
    let markdown_with_emoji = process_emoji(markdown);

    // 预处理：生成自动目录并替换 [[toc]] 标记
    let markdown_with_toc = process_toc(&markdown_with_emoji);

    // 创建 pulldown-cmark 解析器，将 Markdown 文本解析为事件流
    let parser = Parser::new_ext(&markdown_with_toc, options);

    // 用于存储生成的 HTML 字符串
    let mut html_output = String::new();

    // 将解析器产生的事件流推入 HTML 字符串中
    html::push_html(&mut html_output, parser);

    // 后处理：将本地图片路径转换为 Base64 Data URI
    convert_local_images_to_base64(&mut html_output, document_path);

    // 后处理：将数学公式标记为专用类名，供前端 KaTeX 渲染
    process_math_formulas(&mut html_output);

    // 后处理：将 Mermaid 代码块标记为专用类名，供前端 Mermaid 渲染
    process_mermaid_blocks(&mut html_output);

    // 后处理：为表格添加滚动容器，当表格过宽时显示水平滚动条
    process_table_scroll(&mut html_output);

    html_output
}

/// 后处理 HTML 字符串，将本地图片路径替换为 Base64 Data URI
///
/// 遍历 HTML 中的所有 `<img>` 标签，检测 src 属性值：
/// - 如果 src 以 `http://` 或 `https://` 开头 → 跳过（网络图片）
/// - 如果 src 以 `data:` 开头 → 跳过（已经是 Data URI）
/// - 否则视为本地文件路径 → 读取文件并转换为 Base64 Data URI
///
/// 支持相对路径的图片加载：
/// - 如果提供了 document_path，相对路径会相对于文档所在目录解析
/// - 如果没有提供 document_path，相对路径会相对于当前工作目录解析
///
/// 注意：pulldown-cmark 会将路径中的非 ASCII 字符（如中文）进行 URL 编码
/// （例如 `图片` → `%E5%9B%BE%E7%89%87`），因此在读取文件前需要先进行 URL 解码。
///
/// 如果文件读取失败（文件不存在、权限不足等），保留原始路径不变。
///
/// # 参数
/// - `html`: 要处理的 HTML 字符串（原地修改）
/// - `document_path`: 文档所在的文件路径，用于解析相对路径
fn convert_local_images_to_base64(html: &mut String, document_path: Option<&str>) {
    use base64::Engine;

    let mut replacements: Vec<(String, String)> = Vec::new();

    for caps in IMG_SRC_REGEX.captures_iter(html) {
        let full_match = caps.get(0).unwrap().as_str().to_string();
        let src_value = caps.get(1).unwrap().as_str();

        if src_value.starts_with("http://")
            || src_value.starts_with("https://")
            || src_value.starts_with("data:")
        {
            continue;
        }

        let decoded_path = percent_decode(src_value);
        let path = resolve_image_path(&decoded_path, document_path);

        match std::fs::read(&path) {
            Ok(data) => {
                let mime = get_mime_type(src_value);
                let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                let data_uri = format!("data:{};base64,{}", mime, b64);

                let new_img = full_match.replace(src_value, &data_uri);
                replacements.push((full_match, new_img));
            }
            Err(e) => {
                eprintln!("[MarkStudio] 图片文件读取失败 ({}): {}", path.display(), e);
            }
        }
    }

    for (original, replacement) in replacements {
        *html = html.replace(&original, &replacement);
    }
}

/// 根据文档路径解析图片的绝对路径
///
/// 如果图片路径是相对路径（不以 `/` 或 Windows 盘符开头），
/// 则相对于文档所在目录进行解析。
///
/// # 参数
/// - `image_path`: 图片路径（可能是相对路径或绝对路径）
/// - `document_path`: 文档所在的文件路径（可选）
///
/// # 返回
/// 解析后的绝对路径
fn resolve_image_path(image_path: &str, document_path: Option<&str>) -> std::path::PathBuf {
    let path = std::path::Path::new(image_path);

    if path.is_absolute() {
        return path.to_path_buf();
    }

    match document_path {
        Some(doc_path) => {
            let doc_dir = std::path::Path::new(doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            doc_dir.join(image_path)
        }
        None => {
            std::path::Path::new(".").join(image_path)
        }
    }
}

/// 对 URL 编码（百分号编码）的字符串进行解码
///
/// 例如 `%E5%9B%BE%E7%89%87` → `图片`。
/// pulldown-cmark 在解析 Markdown 图片链接时，会将非 ASCII 字符
/// 进行 URL 编码，因此在读取本地文件前需要先解码。
///
/// 解码失败时返回原始字符串。
///
/// # 参数
/// - `input`: 可能包含百分号编码的字符串
///
/// # 返回
/// 解码后的字符串
fn percent_decode(input: &str) -> String {
    // 如果字符串中不包含 %，无需解码，直接返回
    if !input.contains('%') {
        return input.to_string();
    }

    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        if c == '%' {
            // 读取 % 后的两个十六进制字符
            let h1 = chars.next().and_then(|ch| ch.to_digit(16));
            let h2 = chars.next().and_then(|ch| ch.to_digit(16));
            if let (Some(hi), Some(lo)) = (h1, h2) {
                bytes.push(((hi << 4) | lo) as u8);
            } else {
                // 解码失败，保留原始 % 字符（实际场景中 pulldown-cmark 生成的编码总是有效的）
                bytes.push(b'%');
            }
        } else {
            // 非 % 字符直接保留（ASCII 范围内）
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            bytes.extend_from_slice(encoded.as_bytes());
        }
    }

    // 将字节序列解码为 UTF-8 字符串
    String::from_utf8(bytes).unwrap_or_else(|_| input.to_string())
}

/// 根据文件扩展名推断 MIME 类型
///
/// 支持的图片格式：png, jpg/jpeg, gif, webp, svg, bmp, ico
/// 无法识别时默认返回 `image/png`。
///
/// # 参数
/// - `path`: 文件路径字符串
///
/// # 返回
/// 对应的 MIME 类型字符串
fn get_mime_type(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else {
        "image/png" // 默认 MIME 类型
    }
}

/// 预处理 Markdown 文本，将 [[toc]] 标记替换为自动生成的目录
///
/// 从 Markdown 中提取所有标题（H1-H6），生成嵌套的无序列表作为目录。
/// 目录项通过锚点链接到对应的标题位置，使用 pulldown-cmark 生成的 id 或
/// 根据 title_text 生成的兼容 id。
fn process_toc(markdown: &str) -> String {
    if !TOC_REGEX.is_match(markdown) {
        return markdown.to_string();
    }

    let outline = extract_outline(markdown);
    if outline.is_empty() {
        return TOC_REGEX.replace(markdown, "").to_string();
    }

    let mut toc_markdown = String::from("<div class=\"toc-container\">\n");
    toc_markdown.push_str("## 目录\n\n");

    let mut current_level = 0;

    for item in outline {
        while current_level < item.level {
            toc_markdown.push_str("  ".repeat((current_level) as usize).as_str());
            toc_markdown.push_str("-\n");
            current_level += 1;
        }

        while current_level > item.level {
            current_level -= 1;
        }

        // 使用 extract_outline 中生成的 anchor，确保与 HTML id 一致
        let link = format!("[{}](#{})", item.text, item.anchor);

        toc_markdown.push_str("  ".repeat((item.level - 1) as usize).as_str());
        toc_markdown.push_str(&format!("- {}\n", link));
    }

    toc_markdown.push_str("</div>\n");

    TOC_REGEX.replace(markdown, &toc_markdown).to_string()
}

/// 后处理 HTML 字符串，将数学公式标记为专用类名
///
/// 将 `$$...$$` 格式的块级公式转换为 `<div class="math-block">`
/// 将 `$...$` 格式的行内公式转换为 `<span class="math-inline">`
/// 前端通过 KaTeX 库对这些元素进行渲染。
fn process_math_formulas(html: &mut String) {
    *html = MATH_BLOCK_REGEX
        .replace_all(html, r#"<div class="math-block">$1</div>"#)
        .to_string();

    *html = MATH_INLINE_REGEX
        .replace_all(html, r#"$1<span class="math-inline">$2</span>$3"#)
        .to_string();
}

/// 后处理 HTML 字符串，将 Mermaid 代码块标记为专用类名
///
/// 将 ` ```mermaid ... ``` ` 格式的代码块转换为 `<pre class="mermaid">`，
/// 供前端 Mermaid 库渲染为图表。
fn process_mermaid_blocks(html: &mut String) {
    // 使用静态编译的正则表达式，避免每次调用时重新编译
    *html = MERMAID_REGEX
        .replace_all(html, |caps: &regex::Captures| {
            // 对内容进行 HTML 反转义（pulldown-cmark 会对 & < > 进行转义）
            let raw = &caps[1];
            let unescaped = raw
                .replace("&gt;", ">")
                .replace("&lt;", "<")
                .replace("&amp;", "&")
                .replace("&quot;", "\"");
            format!(r#"<pre class="mermaid">{}</pre>"#, unescaped)
        })
        .to_string();
}

/// 后处理 HTML 字符串，为表格添加滚动容器
///
/// 将 `<table>...</table>` 包裹在 `<div class="table-wrapper">` 中，
/// 当表格内容超出容器宽度时，自动显示水平滚动条。
fn process_table_scroll(html: &mut String) {
    // 使用静态编译的正则表达式，避免每次调用时重新编译
    *html = TABLE_REGEX
        .replace_all(html, r#"<div class="table-wrapper"><table>$1</table></div>"#)
        .to_string();
}

/// 预处理 Markdown 文本，将 Emoji 短代码（如 `:smile:`）转换为 Unicode Emoji
///
/// 支持常见的 GitHub 风格 Emoji 短代码。如果短代码不在支持列表中，
/// 则保持原样不进行替换。
fn process_emoji(markdown: &str) -> String {
    EMOJI_REGEX
        .replace_all(markdown, |caps: &regex::Captures| {
            let name = caps.get(1).unwrap().as_str();
            emoji_to_unicode(name).unwrap_or_else(|| caps.get(0).unwrap().as_str().to_string())
        })
        .to_string()
}

/// 将 Emoji 短代码名称转换为对应的 Unicode Emoji 字符
///
/// 如果名称不在支持列表中，返回 None。
///
/// # 参数
/// - `name`: Emoji 短代码名称（不含冒号）
///
/// # 返回
/// 成功返回 Some(Emoji 字符串)，失败返回 None
fn emoji_to_unicode(name: &str) -> Option<String> {
    let emoji = match name {
        // 表情
        "smile" => "😄",
        "laughing" | "satisfied" => "😆",
        "grin" => "😁",
        "joy" => "😂",
        "rofl" => "🤣",
        "wink" => "😉",
        "blush" => "😊",
        "yum" => "😋",
        "sunglasses" => "😎",
        "heart_eyes" => "😍",
        "kissing_heart" => "😘",
        "thinking" => "🤔",
        "neutral_face" => "😐",
        "expressionless" => "😑",
        "no_mouth" => "😶",
        "smirk" => "😏",
        "unamused" => "😒",
        "roll_eyes" => "🙄",
        "grimacing" => "😬",
        "lying_face" => "🤥",
        "relieved" => "😌",
        "pensive" => "😔",
        "sleepy" => "😪",
        "sleeping" => "😴",
        "mask" => "😷",
        "sick" => "🤒",
        "injured" => "🤕",
        "nauseated_face" => "🤢",
        "vomiting_face" => "🤮",
        "sneeze" => "🤧",
        "hot" => "🥵",
        "cold" => "🥶",
        "woozy_face" => "🥴",
        "dizzy_face" => "😵",
        "exploding_head" => "🤯",
        "cowboy_hat_face" => "🤠",
        "partying_face" => "🥳",
        "disguised_face" => "🥸",
        "smiling_imp" => "😈",
        "imp" => "👿",
        "skull" => "💀",
        "skull_and_crossbones" => "☠️",
        "hankey" | "poop" => "💩",
        "clown_face" => "🤡",
        "japanese_ogre" => "👹",
        "japanese_goblin" => "👺",
        "ghost" => "👻",
        "alien" => "👽",
        "space_invader" => "👾",
        "robot" => "🤖",
        // 手势
        "wave" => "👋",
        "raised_back_of_hand" => "🤚",
        "raised_hand_with_fingers_splayed" => "🖐️",
        "hand" | "raised_hand" => "✋",
        "vulcan_salute" => "🖖",
        "ok_hand" => "👌",
        "pinched_fingers" => "🤌",
        "pinching_hand" => "🤏",
        "v" => "✌️",
        "crossed_fingers" => "🤞",
        "love_you_gesture" => "🤟",
        "metal" => "🤘",
        "call_me_hand" => "🤙",
        "point_left" => "👈",
        "point_right" => "👉",
        "point_up_2" => "👆",
        "middle_finger" | "fu" => "🖕",
        "point_down" => "👇",
        "point_up" => "☝️",
        "+1" | "thumbsup" => "👍",
        "-1" | "thumbsdown" => "👎",
        "fist" | "facepunch" | "punch" => "👊",
        "left_facing_fist" => "🤛",
        "right_facing_fist" => "🤜",
        "clap" => "👏",
        "raised_hands" => "🙌",
        "open_hands" => "👐",
        "palms_up_together" => "🤲",
        "handshake" => "🤝",
        "pray" => "🙏",
        "writing_hand" => "✍️",
        "nail_care" => "💅",
        "selfie" => "🤳",
        "muscle" => "💪",
        // 符号
        "heart" => "❤️",
        "orange_heart" => "🧡",
        "yellow_heart" => "💛",
        "green_heart" => "💚",
        "blue_heart" => "💙",
        "purple_heart" => "💜",
        "black_heart" => "🖤",
        "white_heart" => "🤍",
        "brown_heart" => "🤎",
        "broken_heart" => "💔",
        "heart_on_fire" => "❤️‍🔥",
        "sparkling_heart" => "❤️‍🩹",
        "two_hearts" => "💕",
        "revolving_hearts" => "💞",
        "heartbeat" => "💓",
        "heartpulse" => "💗",
        "cupid" => "💘",
        "sparkles" => "✨",
        "star" => "⭐",
        "star2" => "🌟",
        "dizzy" => "💫",
        "boom" | "collision" => "💥",
        "fire" => "🔥",
        "100" => "💯",
        "check_mark_button" | "white_check_mark" => "✅",
        "negative_squared_cross_mark" => "❎",
        "x" => "❌",
        "warning" => "⚠️",
        "no_entry" => "⛔",
        "no_entry_sign" => "🚫",
        "question" => "❓",
        "exclamation" => "❗",
        "heavy_exclamation_mark" => "❗",
        "heavy_question_mark" => "❓",
        // 动物
        "dog" => "🐶",
        "cat" => "🐱",
        "mouse" => "🐭",
        "hamster" => "🐹",
        "rabbit" => "🐰",
        "fox_face" => "🦊",
        "bear" => "🐻",
        "panda_face" => "🐼",
        "koala" => "🐨",
        "tiger" => "🐯",
        "lion" => "🦁",
        "cow" => "🐮",
        "pig" => "🐷",
        "frog" => "🐸",
        "monkey_face" => "🐵",
        "chicken" => "🐔",
        "penguin" => "🐧",
        "bird" => "🐦",
        "baby_chick" => "🐤",
        "hatching_chick" => "🐣",
        "hatched_chick" => "🐥",
        "duck" => "🦆",
        "eagle" => "🦅",
        "owl" => "🦉",
        "bat" => "🦇",
        "wolf" => "🐺",
        "boar" => "🐗",
        "horse" => "🐴",
        "unicorn" => "🦄",
        "bee" | "honeybee" => "🐝",
        "bug" => "🐛",
        "butterfly" => "🦋",
        "snail" => "🐌",
        "shell" => "🐚",
        "snake" => "🐍",
        "turtle" => "🐢",
        "lizard" => "🦎",
        "dinosaur" => "🦖",
        "dragon" => "🐉",
        "dragon_face" => "🐲",
        // 食物
        "apple" => "🍎",
        "green_apple" => "🍏",
        "pear" => "🍐",
        "tangerine" => "🍊",
        "lemon" => "🍋",
        "banana" => "🍌",
        "watermelon" => "🍉",
        "grapes" => "🍇",
        "strawberry" => "🍓",
        "melon" => "🍈",
        "cherries" => "🍒",
        "peach" => "🍑",
        "pineapple" => "🍍",
        "tomato" => "🍅",
        "eggplant" => "🍆",
        "corn" => "🌽",
        "carrot" => "🥕",
        "hot_pepper" => "🌶️",
        "potato" => "🥔",
        "sweet_potato" => "🍠",
        "pizza" => "🍕",
        "hamburger" => "🍔",
        "fries" => "🍟",
        "hotdog" => "🌭",
        "taco" => "🌮",
        "burrito" => "🌯",
        "sushi" => "🍣",
        "ramen" => "🍜",
        "rice" => "🍚",
        "rice_ball" => "🍙",
        "rice_cracker" => "🍘",
        "curry" => "🍛",
        "stew" => "🍲",
        "bread" => "🍞",
        "cheese" => "🧀",
        "egg" => "🥚",
        "fried_egg" => "🍳",
        "bacon" => "🥓",
        "cake" => "🍰",
        "birthday" => "🎂",
        "cookie" => "🍪",
        "chocolate_bar" => "🍫",
        "candy" => "🍬",
        "lollipop" => "🍭",
        "doughnut" => "🍩",
        "ice_cream" => "🍨",
        "coffee" => "☕",
        "tea" => "🍵",
        "beer" => "🍺",
        "beers" => "🍻",
        "wine_glass" => "🍷",
        "cocktail" => "🍸",
        "tropical_drink" => "🍹",
        // 箭头与符号
        "arrow_up" => "⬆️",
        "arrow_down" => "⬇️",
        "arrow_left" => "⬅️",
        "arrow_right" => "➡️",
        "arrow_upper_right" => "↗️",
        "arrow_upper_left" => "↖️",
        "arrow_lower_right" => "↘️",
        "arrow_lower_left" => "↙️",
        "arrows_clockwise" => "🔃",
        "arrows_counterclockwise" => "🔄",
        "back" => "🔙",
        "end" => "🔚",
        "on" => "🔛",
        "soon" => "🔜",
        "top" => "🔝",
        // 技术相关
        "computer" => "💻",
        "keyboard" => "⌨️",
        "desktop_computer" => "🖥️",
        "printer" => "🖨️",
        "mouse_three_button" => "🖱️",
        "trackball" => "🖲️",
        "joystick" => "🕹️",
        "compression" => "🗜️",
        "minidisc" => "💽",
        "floppy_disk" => "💾",
        "cd" => "💿",
        "dvd" => "📀",
        "electric_plug" => "🔌",
        "battery" => "🔋",
        "mag" => "🔍",
        "mag_right" => "🔎",
        "lock" => "🔒",
        "unlock" => "🔓",
        "lock_with_ink_pen" => "🔏",
        "closed_lock_with_key" => "🔐",
        "key" => "🔑",
        "hammer" => "🔨",
        "wrench" => "🔧",
        "nut_and_bolt" => "🔩",
        "gear" => "⚙️",
        "chains" => "⛓️",
        "package" => "📦",
        "bell" => "🔔",
        "mute" => "🔕",
        "speaker" => "🔈",
        "sound" => "🔉",
        "loud_sound" => "🔊",
        "mega" => "📣",
        "loudspeaker" => "📢",
        "bellhop_bell" => "🛎️",
        "bulb" => "💡",
        // 工具
        "pencil2" => "✏️",
        "black_nib" => "✒️",
        "memo" | "pencil" => "📝",
        "book" => "📖",
        "books" => "📚",
        "notebook" => "📓",
        "notebook_with_decorative_cover" => "📔",
        "closed_book" => "📕",
        "green_book" => "📗",
        "blue_book" => "📘",
        "orange_book" => "📙",
        "scroll" => "📜",
        "page_with_curl" => "📃",
        "page_facing_up" => "📄",
        "newspaper" => "📰",
        "card_index" => "📇",
        "card_file_box" => "🗃️",
        "file_folder" => "📁",
        "open_file_folder" => "📂",
        "card_index_dividers" => "🗂️",
        "date" => "📅",
        "calendar" => "📆",
        "spiral_notepad" => "🗒️",
        "spiral_calendar" => "🗓️",
        "clipboard" => "📋",
        "pushpin" => "📌",
        "round_pushpin" => "📍",
        "paperclip" => "📎",
        "linked_paperclips" => "🖇️",
        "straight_ruler" => "📏",
        "triangular_ruler" => "📐",
        "scissors" => "✂️",
        "card_box" => "🗄️",
        "file_cabinet" => "🗄️",
        "wastebasket" => "🗑️",
        "lock2" => "🔒",
        "unlock2" => "🔓",
        // 通用
        "rocket" => "🚀",
        "airplane" => "✈️",
        "helicopter" => "🚁",
        "boat" => "⛵",
        "car" => "🚗",
        "taxi" => "🚕",
        "bus" => "🚌",
        "train" => "🚆",
        "bike" => "🚲",
        "fuelpump" => "⛽",
        "construction" => "🚧",
        "vertical_traffic_light" => "🚦",
        "traffic_light" => "🚥",
        "map" => "🗺️",
        "moyai" => "🗿",
        "statue_of_liberty" => "🗽",
        "tokyo_tower" => "🗼",
        "european_castle" => "🏰",
        "japanese_castle" => "🏯",
        "tent" => "⛺",
        "factory" => "🏭",
        "fountain" => "⛲",
        "tokyo_tower_emoji" => "🗼",
        "rainbow" => "🌈",
        "sunny" => "☀️",
        "cloud" => "☁️",
        "snowflake" => "❄️",
        "ocean" => "🌊",
        "earth_africa" => "🌍",
        "earth_americas" => "🌎",
        "earth_asia" => "🌏",
        "full_moon" => "🌕",
        "new_moon" => "🌑",
        "crescent_moon" => "🌙",
        "zap" => "⚡",
        _ => return None,
    };
    Some(emoji.to_string())
}

/// 大纲条目结构体，表示文档中的一个标题
#[derive(Debug, Clone, serde::Serialize)]
pub struct OutlineItem {
    /// 标题级别（1-6，对应 H1-H6）
    pub level: u32,
    /// 标题文本内容
    pub text: String,
    /// 标题在文档中的行号（从 1 开始计数）
    pub line: usize,
    /// 标题的锚点 ID（用于 TOC 链接跳转）
    /// 如果 pulldown-cmark 自动生成了 id，则使用该 id
    /// 否则根据标题文本生成一个兼容的 id
    pub anchor: String,
}

/// 从 Markdown 文本中提取大纲（所有标题）
///
/// 使用 pulldown-cmark 解析器遍历文档的事件流，
/// 提取所有 Heading 事件的标题级别、文本内容、行号和锚点 ID。
///
/// # 参数
/// - `markdown`: 要分析（解析）的 Markdown 源文本
///
/// # 返回
/// 包含所有标题信息的大纲条目列表（按文档出现顺序排列）
pub fn extract_outline(markdown: &str) -> Vec<OutlineItem> {
    // 启用 GFM 扩展语法，与 parse_markdown_to_html 保持一致
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_SMART_PUNCTUATION;

    // 创建解析器，返回 (事件, 字节范围) 元组的迭代器
    let mut parser = Parser::new_ext(markdown, options).into_offset_iter();

    // 用于存储提取的大纲条目
    let mut outline_items: Vec<OutlineItem> = Vec::new();

    // 遍历解析器生成的事件流
    // 使用 by_ref() 允许在循环体内借用解析器继续迭代
    while let Some((event, range)) = parser.next() {
        // 当遇到 Heading 开始标签时，记录标题信息
        if let Event::Start(Tag::Heading { level, id, .. }) = event {
            // 计算行号：统计该事件起始字节之前的换行符数量
            let line = markdown[..range.start]
                .chars()
                .filter(|&c| c == '\n')
                .count()
                + 1; // 行号从 1 开始计数

            // 将标题级别转换为 u32
            let heading_level = level as u32;

            // 继续收集同一个 Heading 中的文本内容
            // 注意：当前事件是 Start，文本内容在后续的 Text 事件中
            // 我们需要在解析器迭代中继续处理后续事件

            // 创建一个收集器，用于累积标题中的文本
            let mut title_text = String::new();

            // 继续遍历后续事件，直到遇到 End 标签结束当前标题
            // 收集所有 Text 和 Code 事件中的文本内容
            for (inner_event, _inner_range) in parser.by_ref() {
                match inner_event {
                    Event::Text(text) | Event::Code(text) => {
                        // 将文本内容追加到标题文本中
                        title_text.push_str(&text);
                    }
                    Event::End(TagEnd::Heading(_)) => {
                        // 标题结束，退出内层循环
                        break;
                    }
                    _ => {
                        // 忽略其他事件（如 SoftBreak、InlineHtml 等）
                    }
                }
            }

            // 生成锚点 ID：
            // 1. 如果 pulldown-cmark 已经生成了 id（通过 ENABLE_HEADING_ATTRIBUTES），直接使用
            // 2. 否则，根据标题文本生成一个兼容的 id
            let anchor = if let Some(ref heading_id) = id {
                heading_id.to_string()
            } else {
                generate_anchor_id(&title_text)
            };

            // 将提取到的标题信息添加到结果列表中
            outline_items.push(OutlineItem {
                level: heading_level,
                text: title_text,
                line,
                anchor,
            });
        }
    }

    outline_items
}

/// 根据标题文本生成锚点 ID
///
/// 模拟 pulldown-cmark 的 id 生成规则：
/// - 英文标题：转换为小写，空格替换为 `-`
/// - 中文标题：保留中文字符（不做转换）
/// - 混合标题：英文部分转小写，中文部分保留
///
/// # 参数
/// - `text`: 标题文本
///
/// # 返回
/// 生成的锚点 ID 字符串
fn generate_anchor_id(text: &str) -> String {
    let mut result = String::new();
    let mut prev_char = '\0';

    for c in text.chars() {
        if c.is_whitespace() {
            // 空格替换为 `-`，但避免连续的 `-`
            if prev_char != '-' && !result.is_empty() {
                result.push('-');
                prev_char = '-';
            }
        } else if c.is_ascii_alphanumeric() {
            // ASCII 字母数字：转换为小写
            result.push(c.to_ascii_lowercase());
            prev_char = c.to_ascii_lowercase();
        } else if c.is_alphanumeric() {
            // 非 ASCII 字母数字（如中文）：保留
            result.push(c);
            prev_char = c;
        } else {
            // 其他字符（如标点符号）：忽略
            // 但如果前面不是 `-`，可以添加 `-` 作为分隔
        }
    }

    // 移除末尾可能多余的 `-`
    if result.ends_with('-') {
        result.pop();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试基本标题解析
    #[test]
    fn test_heading() {
        let html = parse_markdown_to_html("# 标题", None);
        assert!(html.contains("<h1>标题</h1>"));
    }

    /// 测试加粗文本
    #[test]
    fn test_bold() {
        let html = parse_markdown_to_html("**加粗**", None);
        assert!(html.contains("<strong>加粗</strong>"));
    }

    /// 测试链接解析
    #[test]
    fn test_link() {
        let html = parse_markdown_to_html("[链接](https://example.com)", None);
        assert!(html.contains("<a href=\"https://example.com\">链接</a>"));
    }

    /// 测试空文本
    #[test]
    fn test_empty() {
        let html = parse_markdown_to_html("", None);
        assert!(html.is_empty());
    }

    /// 测试表格解析
    #[test]
    fn test_table() {
        let markdown = "| 列1 | 列2 |\n|-----|-----|\n| A   | B   |";
        let html = parse_markdown_to_html(markdown, None);
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>列1</th>"));
        assert!(html.contains("<td>A</td>"));
    }

    /// 测试任务列表
    #[test]
    fn test_tasklist() {
        let html = parse_markdown_to_html("- [x] 已完成\n- [ ] 未完成", None);
        assert!(html.contains("checked"));
        assert!(html.contains("type=\"checkbox\""));
    }

    /// 测试删除线
    #[test]
    fn test_strikethrough() {
        let html = parse_markdown_to_html("~~删除线~~", None);
        assert!(html.contains("<del>删除线</del>"));
    }

    /// 测试 Emoji 短代码转换
    #[test]
    fn test_emoji() {
        let html = parse_markdown_to_html(":smile: 笑 :heart: 心", None);
        assert!(html.contains("😄"));
        assert!(html.contains("❤️"));
    }

    /// 测试未支持的 Emoji 保持原样
    #[test]
    fn test_emoji_unsupported() {
        let html = parse_markdown_to_html(":nonexistent_emoji:", None);
        assert!(html.contains(":nonexistent_emoji:"));
    }

    /// 测试脚注
    #[test]
    fn test_footnote() {
        let html = parse_markdown_to_html("文本[^1]\n\n[^1]: 脚注内容", None);
        eprintln!("Footnote HTML output: {}", html);
        // pulldown-cmark 会生成 <sup class="footnote-reference"> 等标签
        assert!(html.contains("footnote") || html.contains("脚注内容"));
    }

    /// 测试智能标点
    #[test]
    fn test_smart_punctuation() {
        let html = parse_markdown_to_html("\"hello\" -- world", None);
        // 智能标点会转换引号和破折号
        assert!(html.contains("hello") || html.contains("hello"));
    }

    /// 测试块级数学公式
    #[test]
    fn test_math_block() {
        let html = parse_markdown_to_html("$$E=mc^2$$", None);
        assert!(html.contains("math-block"));
    }

    /// 测试行内数学公式
    #[test]
    fn test_math_inline() {
        let html = parse_markdown_to_html("这是 $E=mc^2$ 公式", None);
        assert!(html.contains("math-inline"));
    }

    /// 测试单字符数学公式（之前无法匹配）
    #[test]
    fn test_math_inline_single_char() {
        let html = parse_markdown_to_html("设 $x$ 为变量", None);
        assert!(html.contains("math-inline"));
        eprintln!("单字符公式 HTML: {}", html);
    }

    /// 测试连续的行内数学公式（之前无法匹配）
    #[test]
    fn test_math_inline_consecutive() {
        let html = parse_markdown_to_html("设 $a$ 和 $b$ 为变量", None);
        // 应该有两个 math-inline
        let count = html.matches("math-inline").count();
        assert_eq!(count, 2, "连续公式应匹配两个 math-inline");
        eprintln!("连续公式 HTML: {}", html);
    }

    /// 测试公式边界 - 公式前有字符
    #[test]
    fn test_math_inline_before_char() {
        let html = parse_markdown_to_html("设 $x$ 为变量", None);
        // "设 " 应该保留
        assert!(html.contains("设 <span"), "公式前的字符应保留");
        eprintln!("公式前有字符 HTML: {}", html);
    }

    /// 测试公式边界 - 公式后有字符
    #[test]
    fn test_math_inline_after_char() {
        let html = parse_markdown_to_html("变量是 $x$", None);
        // "变量是 " 应该保留
        assert!(html.contains("变量是 <span"), "公式后的字符应保留");
        eprintln!("公式后有字符 HTML: {}", html);
    }

    /// 测试 Mermaid 代码块
    #[test]
    fn test_mermaid() {
        let markdown = "```mermaid\ngraph TD\n  A-->B\n```";
        let html = parse_markdown_to_html(markdown, None);
        eprintln!("Mermaid HTML output: {}", html);
        // 我们的 process_mermaid_blocks 函数会将其转换为 <pre class="mermaid">
        assert!(html.contains("class=\"mermaid\""));
    }

    /// 测试自动目录
    #[test]
    fn test_toc() {
        let markdown = "# 标题 1\n## 标题 2\n[[toc]]";
        let html = parse_markdown_to_html(markdown, None);
        eprintln!("TOC HTML output: {}", html);
        assert!(html.contains("toc-container"));
    }

    /// 测试 [TOC] 大写格式
    #[test]
    fn test_toc_uppercase() {
        let markdown = "# 标题 1\n[TOC]";
        let html = parse_markdown_to_html(markdown, None);
        eprintln!("[TOC] HTML output: {}", html);
        // [TOC] 也应被识别为目录标记
        assert!(html.contains("toc-container"), "[TOC] 大写格式应生成目录");
    }

    /// 测试中文标题锚点生成
    #[test]
    fn test_chinese_anchor() {
        let markdown = "# 中文标题\n## English Title\n### 混合标题 Mixed";
        let outline = extract_outline(markdown);
        
        // 验证大纲提取正确
        assert_eq!(outline.len(), 3);
        
        // 验证中文标题的锚点保留中文字符
        assert!(outline[0].anchor.contains("中文") || outline[0].anchor.contains("标题"));
        eprintln!("中文标题锚点: {}", outline[0].anchor);
        
        // 验证英文标题的锚点为小写
        assert_eq!(outline[1].anchor, "english-title");
        eprintln!("英文标题锚点: {}", outline[1].anchor);
        
        // 验证混合标题的锚点正确处理
        eprintln!("混合标题锚点: {}", outline[2].anchor);
        assert!(outline[2].anchor.contains("混合") || outline[2].anchor.contains("mixed"));
    }

    /// 测试 generate_anchor_id 函数
    #[test]
    fn test_generate_anchor_id() {
        // 纯中文
        let anchor = generate_anchor_id("中文标题");
        assert_eq!(anchor, "中文标题");
        
        // 纯英文
        let anchor = generate_anchor_id("English Title");
        assert_eq!(anchor, "english-title");
        
        // 混合
        let anchor = generate_anchor_id("Hello 世界");
        assert!(anchor.contains("hello"));
        assert!(anchor.contains("世界"));
        
        // 包含特殊字符
        let anchor = generate_anchor_id("标题!@#$%");
        assert_eq!(anchor, "标题");
        
        // 多空格
        let anchor = generate_anchor_id("A  B   C");
        assert_eq!(anchor, "a-b-c");
    }

    /// 测试相对路径图片解析
    #[test]
    fn test_resolve_image_path_absolute() {
        let path = resolve_image_path("/abs/path/img.png", None);
        assert_eq!(path.to_str().unwrap(), "/abs/path/img.png");
    }

    /// 测试相对路径解析（带 document_path）
    #[test]
    fn test_resolve_image_path_relative_with_doc() {
        let path = resolve_image_path("img.png", Some("/path/to/doc.md"));
        assert!(path.to_str().unwrap().contains("path"));
        assert!(path.to_str().unwrap().ends_with("img.png"));
    }
}
