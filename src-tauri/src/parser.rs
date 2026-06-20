//! Markdown 解析模块
//!
//! 本模块负责将 Markdown 文本转换为 HTML 字符串。
//! 使用 pulldown-cmark 库进行解析，支持表格、任务列表、删除线等扩展语法。

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

/// 将 Markdown 文本解析为 HTML 字符串
///
/// # 参数
/// - `markdown`: 要解析的 Markdown 源文本
///
/// # 返回
/// 解析后生成的 HTML 字符串
///
/// # 支持的扩展语法
/// - 表格 (table)
/// - 任务列表 (tasklist)
/// - 删除线 (strikethrough)
pub fn parse_markdown_to_html(markdown: &str) -> String {
    // 启用 GFM 扩展语法：表格、任务列表、删除线
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH;

    // 创建 pulldown-cmark 解析器，将 Markdown 文本解析为事件流
    let parser = Parser::new_ext(markdown, options);

    // 用于存储生成的 HTML 字符串
    let mut html_output = String::new();

    // 将解析器产生的事件流推入 HTML 字符串中
    html::push_html(&mut html_output, parser);

    html_output
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
}

/// 从 Markdown 文本中提取大纲（所有标题）
///
/// 使用 pulldown-cmark 解析器遍历文档的事件流，
/// 提取所有 Heading 事件的标题级别、文本内容和行号。
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
        | Options::ENABLE_STRIKETHROUGH;

    // 创建解析器，返回 (事件, 字节范围) 元组的迭代器
    let mut parser = Parser::new_ext(markdown, options).into_offset_iter();

    // 用于存储提取的大纲条目
    let mut outline_items: Vec<OutlineItem> = Vec::new();

    // 遍历解析器生成的事件流
    // 使用 by_ref() 允许在循环体内借用解析器继续迭代
    while let Some((event, range)) = parser.next() {
        // 当遇到 Heading 开始标签时，记录标题信息
        if let Event::Start(Tag::Heading { level, .. }) = event {
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

            // 将提取到的标题信息添加到结果列表中
            outline_items.push(OutlineItem {
                level: heading_level,
                text: title_text,
                line,
            });
        }
    }

    outline_items
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试基本标题解析
    #[test]
    fn test_heading() {
        let html = parse_markdown_to_html("# 标题");
        assert!(html.contains("<h1>标题</h1>"));
    }

    /// 测试加粗文本
    #[test]
    fn test_bold() {
        let html = parse_markdown_to_html("**加粗**");
        assert!(html.contains("<strong>加粗</strong>"));
    }

    /// 测试链接解析
    #[test]
    fn test_link() {
        let html = parse_markdown_to_html("[链接](https://example.com)");
        assert!(html.contains("<a href=\"https://example.com\">链接</a>"));
    }

    /// 测试空文本
    #[test]
    fn test_empty() {
        let html = parse_markdown_to_html("");
        assert!(html.is_empty());
    }

    /// 测试表格解析
    #[test]
    fn test_table() {
        let markdown = "| 列1 | 列2 |\n|-----|-----|\n| A   | B   |";
        let html = parse_markdown_to_html(markdown);
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>列1</th>"));
        assert!(html.contains("<td>A</td>"));
    }

    /// 测试任务列表
    #[test]
    fn test_tasklist() {
        let html = parse_markdown_to_html("- [x] 已完成\n- [ ] 未完成");
        assert!(html.contains("checked"));
        assert!(html.contains("type=\"checkbox\""));
    }

    /// 测试删除线
    #[test]
    fn test_strikethrough() {
        let html = parse_markdown_to_html("~~删除线~~");
        assert!(html.contains("<del>删除线</del>"));
    }
}