//! 文档状态管理服务
//!
//! 提供文档脏状态检测和字符位置计算等纯函数式服务接口。

/// 检查文档是否处于"脏"状态（即当前内容与已保存内容是否不同）
///
/// # 参数
/// - `current_content`: 当前编辑器中的文档内容
/// - `saved_content`: 上次保存时的文档内容
///
/// # 返回值
/// 如果两个字符串内容不同则返回 `true`，相同则返回 `false`
pub fn check_dirty(current_content: &str, saved_content: &str) -> bool {
    current_content != saved_content
}

/// 根据文档内容计算指定行号（1-based）的起始字符偏移量
///
/// # 参数
/// - `content`: 文档的完整文本内容
/// - `line_number`: 目标行号，从 1 开始计数
///
/// # 返回值
/// 返回目标行首个字符在整个文档中的字符偏移量（字节偏移）。
/// 如果 `line_number` 超出文档的实际行数范围，则返回最后一行的起始位置。
///
/// # 示例
/// ```
/// let content = "abc\ndef\nghi";
/// assert_eq!(compute_line_position(content, 1), 0);  // 第一行从位置 0 开始
/// assert_eq!(compute_line_position(content, 2), 4);  // 第二行从位置 4 开始（"abc" + '\n'）
/// assert_eq!(compute_line_position(content, 3), 8);  // 第三行从位置 8 开始
/// ```
pub fn compute_line_position(content: &str, line_number: usize) -> usize {
    // 行号必须至少为 1
    if line_number == 0 {
        return 0;
    }

    let mut offset = 0usize;
    let mut current_line = 1usize;

    // 遍历每一行，累加行长度和换行符
    for line in content.split('\n') {
        if current_line >= line_number {
            // 已到达目标行，返回当前累积的偏移量
            return offset;
        }
        // 累加当前行的字节长度 + 1（换行符 `\n` 占 1 字节）
        offset += line.len() + 1;
        current_line += 1;
    }

    // 行号超出范围，计算最后一行的起始位置
    // offset 此时已累加了所有行的长度和所有换行符计数（包括最后一行后面的虚拟换行符）
    // 最后一行的起始位置 = offset - 最后一行长度 - 1（多余的换行符计数）
    if let Some(last_line) = content.split('\n').next_back() {
        offset - last_line.len() - 1
    } else {
        // 理论上不会走到这里，split 至少返回一个元素
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：相同内容应返回 false（非脏状态）
    #[test]
    fn test_check_dirty_same() {
        let content = "Hello, world!";
        assert!(!check_dirty(content, content));
    }

    /// 测试：不同内容应返回 true（脏状态）
    #[test]
    fn test_check_dirty_different() {
        let current = "Hello, world!";
        let saved = "Hello, Rust!";
        assert!(check_dirty(current, saved));
    }

    /// 测试：两个空字符串应返回 false（非脏状态）
    #[test]
    fn test_check_dirty_empty() {
        assert!(!check_dirty("", ""));
    }

    /// 测试：第 1 行的起始偏移量应为 0
    #[test]
    fn test_compute_line_position_first() {
        let content = "abc\ndef\nghi";
        assert_eq!(compute_line_position(content, 1), 0);
    }

    /// 测试：第 2 行的起始偏移量应为 4（"abc" 3 字节 + '\n' 1 字节）
    #[test]
    fn test_compute_line_position_second() {
        let content = "abc\ndef\nghi";
        assert_eq!(compute_line_position(content, 2), 4);
    }

    /// 测试：第 3 行的起始偏移量应为 8（前两行共 8 字节）
    #[test]
    fn test_compute_line_position_third() {
        let content = "abc\ndef\nghi";
        assert_eq!(compute_line_position(content, 3), 8);
    }

    /// 测试：行号超出范围时，应返回最后一个有效位置的偏移量
    #[test]
    fn test_compute_line_position_out_of_range() {
        let content = "abc\ndef\nghi";
        // 只有 3 行，请求第 10 行应返回最后一行的起始位置（8）
        assert_eq!(compute_line_position(content, 10), 8);
    }
}
