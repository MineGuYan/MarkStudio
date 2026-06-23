//! OT 操作服务：计算和应用 Operational Transformation 操作
//!
//! 本模块提供基于文本差异计算 OT 操作（Insert / Delete）的功能，
//! 以及将这些操作应用到文本上的能力。
//! 与 `collaboration::ot` 模块不同，本模块专注于单次编辑的差异计算，
//! 而非并发操作之间的冲突变换。

use serde::{Deserialize, Serialize};

/// 操作类型枚举，表示对文本的一次编辑操作。
///
/// 支持两种原子操作：
/// - `Insert`: 在指定位置插入文本
/// - `Delete`: 从指定位置删除指定长度的文本
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Operation {
    /// 插入操作：在 `position` 处插入 `text` 字符串
    Insert {
        /// 插入位置（基于字符的偏移量，而非字节偏移量）
        position: usize,
        /// 要插入的文本内容
        text: String,
    },
    /// 删除操作：从 `position` 开始，删除 `length` 个字符
    Delete {
        /// 删除起始位置（基于字符的偏移量，而非字节偏移量）
        position: usize,
        /// 要删除的字符数量
        length: usize,
    },
}

/// 计算两个文本之间的差异，生成一组 OT 操作。
///
/// 本函数通过比较 `old_text` 和 `new_text`，找出将旧文本转换为新文本
/// 所需的最小操作集合。算法采用"公共前缀 + 公共后缀"策略：
///
/// 1. 从开头逐字符比较，找到第一个不同的位置（公共前缀）
/// 2. 从末尾逐字符比较剩余部分，找到公共后缀
/// 3. 中间不匹配的部分即为差异：
///    - 旧文本中间部分 → 生成 Delete 操作
///    - 新文本中间部分 → 生成 Insert 操作
///
/// 所有位置计算均基于 Unicode 字符数（而非字节数），
/// 确保对中文、emoji 等多字节字符正确处理。
///
/// # 参数
/// - `old_text`: 编辑前的原始文本
/// - `new_text`: 编辑后的新文本
///
/// # 返回
/// 一个操作向量，按 Delete 优先、Insert 其次的顺序排列：
/// - 纯插入：返回 `[Insert { ... }]`
/// - 纯删除：返回 `[Delete { ... }]`
/// - 替换（先删后插）：返回 `[Delete { ... }, Insert { ... }]`
/// - 无变化：返回空向量 `[]`
///
/// # 示例
/// ```
/// use markstudio_lib::services::ot_service::{compute_operation, Operation};
/// let ops = compute_operation("abc", "abxc");
/// assert_eq!(ops.len(), 1);
/// // ops[0] = Insert { position: 2, text: "x" }
/// ```
pub fn compute_operation(old_text: &str, new_text: &str) -> Vec<Operation> {
    // 将字符串转换为字符向量，以便按字符（而非字节）进行操作
    let old_chars: Vec<char> = old_text.chars().collect();
    let new_chars: Vec<char> = new_text.chars().collect();

    let old_len = old_chars.len();
    let new_len = new_chars.len();

    // ----------------------------------------------------------------
    // 第一步：找到公共前缀长度
    // 从开头逐字符比较，直到遇到第一个不同的字符
    // ----------------------------------------------------------------
    let common_prefix = old_chars
        .iter()
        .zip(new_chars.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // 如果公共前缀完全覆盖了两个字符串，说明文本完全相同
    if common_prefix == old_len && common_prefix == new_len {
        return vec![];
    }

    // ----------------------------------------------------------------
    // 第二步：在剩余部分中，从末尾找到公共后缀长度
    // 剩余部分 = 去掉公共前缀后的部分
    // ----------------------------------------------------------------
    let old_remaining = &old_chars[common_prefix..];
    let new_remaining = &new_chars[common_prefix..];

    let common_suffix = old_remaining
        .iter()
        .rev()
        .zip(new_remaining.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();

    // ----------------------------------------------------------------
    // 第三步：中间不匹配的部分即为差异
    // ----------------------------------------------------------------
    let old_mid_len = old_remaining.len() - common_suffix;
    let new_mid_len = new_remaining.len() - common_suffix;

    let mut ops = Vec::new();

    // 如果旧文本中间部分非空，生成 Delete 操作（先删除旧内容）
    if old_mid_len > 0 {
        ops.push(Operation::Delete {
            position: common_prefix,
            length: old_mid_len,
        });
    }

    // 如果新文本中间部分非空，生成 Insert 操作（再插入新内容）
    if new_mid_len > 0 {
        let new_text_mid: String = new_remaining[..new_mid_len].iter().collect();
        ops.push(Operation::Insert {
            position: common_prefix,
            text: new_text_mid,
        });
    }

    ops
}

/// 计算字符串的字符数（Unicode 标量值数量），而非字节长度。
///
/// 对于多字节 UTF-8 字符（如中文、emoji），字符数小于字节长度。
/// 例如："你好" 的字节长度为 6，但字符数为 2。
#[inline]
fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// 将基于字符的偏移量转换为基于字节的偏移量，用于安全的字符串切片。
///
/// Rust 字符串切片必须位于有效的 UTF-8 字符边界上，
/// 直接使用字符偏移量进行切片可能导致 panic。
/// 本函数通过遍历字符索引，将字符位置转换为安全的字节位置。
///
/// # 参数
/// - `s`: 源字符串
/// - `char_pos`: 字符偏移量（0 表示第一个字符之前）
///
/// # 返回
/// 对应的字节偏移量。如果 `char_pos` 超出字符串总字符数，则返回字符串的字节长度。
fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// 将单个操作应用到文本上，返回修改后的文本。
///
/// 所有位置计算均基于字符偏移量，内部自动转换为字节偏移量
/// 以确保在多字节 UTF-8 字符边界上的安全切片。
///
/// # 参数
/// - `text`: 原始文本
/// - `op`: 要应用的操作
///
/// # 返回
/// 应用操作后的新文本
///
/// # 示例
/// ```
/// use markstudio_lib::services::ot_service::{apply_operation, Operation};
/// let text = "Hello World";
/// let op = Operation::Insert {
///     position: 6,
///     text: "Beautiful ".to_string(),
/// };
/// assert_eq!(apply_operation(text, &op), "Hello Beautiful World");
/// ```
pub fn apply_operation(text: &str, op: &Operation) -> String {
    match op {
        Operation::Insert {
            position,
            text: insert_text,
        } => {
            // 插入操作：在指定字符位置之前插入文本
            let text_char_count = char_count(text);
            if *position >= text_char_count {
                // 插入位置在文本末尾或之后，直接追加
                let mut result = text.to_string();
                result.push_str(insert_text);
                result
            } else {
                // 在文本中间插入：将文本分割为前后两部分，中间插入新文本
                let mut result = String::with_capacity(text.len() + insert_text.len());
                // 将字符位置转换为字节位置，确保在多字节 UTF-8 字符边界上分割
                let byte_pos = char_to_byte_pos(text, *position);
                // 前半部分：position 之前的字符
                result.push_str(&text[..byte_pos]);
                // 插入的文本
                result.push_str(insert_text);
                // 后半部分：position 及之后的字符
                result.push_str(&text[byte_pos..]);
                result
            }
        }
        Operation::Delete { position, length } => {
            // 删除操作：从指定字符位置删除指定字符数量的文本
            let text_char_count = char_count(text);
            if *position >= text_char_count {
                // 删除起始位置超出文本范围，直接返回原文本
                return text.to_string();
            }
            // 计算实际删除的结束字符位置（不包含），确保不超出文本范围
            let char_end = (*position + *length).min(text_char_count);
            // 将字符位置转换为字节位置，确保在多字节 UTF-8 字符边界上分割
            let byte_pos = char_to_byte_pos(text, *position);
            let byte_end = char_to_byte_pos(text, char_end);
            // 拼接删除范围之前和之后的部分
            let mut result = String::with_capacity(text.len() - (byte_end - byte_pos));
            result.push_str(&text[..byte_pos]);
            result.push_str(&text[byte_end..]);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ================================================================
    // compute_operation 测试
    // ================================================================

    /// 测试纯插入场景：在文本中间插入一个字符
    /// old="abc", new="abxc" → Insert at position 2 with text "x"
    #[test]
    fn test_insert() {
        let ops = compute_operation("abc", "abxc");
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            Operation::Insert {
                position: 2,
                text: "x".to_string(),
            }
        );
    }

    /// 测试纯删除场景：从文本中间删除一个字符
    /// old="abc", new="ac" → Delete at position 1 with length 1
    #[test]
    fn test_delete() {
        let ops = compute_operation("abc", "ac");
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            Operation::Delete {
                position: 1,
                length: 1,
            }
        );
    }

    /// 测试无变化场景：新旧文本完全相同
    /// old="abc", new="abc" → None（空向量）
    #[test]
    fn test_no_change() {
        let ops = compute_operation("abc", "abc");
        assert!(ops.is_empty());
    }

    /// 测试从空文本插入：空字符串变为非空文本
    /// old="", new="hello" → Insert at position 0 with text "hello"
    #[test]
    fn test_empty_to_text() {
        let ops = compute_operation("", "hello");
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            Operation::Insert {
                position: 0,
                text: "hello".to_string(),
            }
        );
    }

    /// 测试将文本清空：非空文本变为空字符串
    /// old="hello", new="" → Delete at position 0 with length 5
    #[test]
    fn test_text_to_empty() {
        let ops = compute_operation("hello", "");
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            Operation::Delete {
                position: 0,
                length: 5,
            }
        );
    }

    /// 测试中文文本插入：验证多字节字符的正确位置计算
    /// old="你好世界", new="你好美丽世界" → Insert at position 2 with text "美丽"
    #[test]
    fn test_chinese_text() {
        let ops = compute_operation("你好世界", "你好美丽世界");
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            Operation::Insert {
                position: 2,
                text: "美丽".to_string(),
            }
        );
    }

    /// 测试 emoji 字符插入：验证 4 字节 emoji 字符的正确位置计算
    /// old="a😀c", new="a😀bc" → Insert at position 2 with text "b"
    /// 注意：😀 是一个 Unicode 字符（占 4 字节），但按字符数计算为 1
    #[test]
    fn test_emoji() {
        let ops = compute_operation("a😀c", "a😀bc");
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            Operation::Insert {
                position: 2,
                text: "b".to_string(),
            }
        );
    }

    /// 测试替换场景：相同长度但内容不同（先删后插）
    /// old="abc", new="axc" → Delete at position 1 with length 1 + Insert at position 1 with text "x"
    #[test]
    fn test_replacement() {
        let ops = compute_operation("abc", "axc");
        assert_eq!(ops.len(), 2);
        // 先删除旧字符 'b'
        assert_eq!(
            ops[0],
            Operation::Delete {
                position: 1,
                length: 1,
            }
        );
        // 再插入新字符 'x'
        assert_eq!(
            ops[1],
            Operation::Insert {
                position: 1,
                text: "x".to_string(),
            }
        );
    }

    // ================================================================
    // apply_operation 测试
    // ================================================================

    /// 测试应用 Insert 操作：在空文本开头插入
    #[test]
    fn test_apply_insert() {
        let text = "";
        let op = Operation::Insert {
            position: 0,
            text: "Hello".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hello");
    }

    /// 测试应用 Insert 操作：在文本中间插入
    #[test]
    fn test_apply_insert_middle() {
        let text = "HelloWorld";
        let op = Operation::Insert {
            position: 5,
            text: " ".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hello World");
    }

    /// 测试应用 Insert 操作：在文本末尾插入
    #[test]
    fn test_apply_insert_at_end() {
        let text = "Hello";
        let op = Operation::Insert {
            position: 5,
            text: " World".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hello World");
    }

    /// 测试应用 Delete 操作：从文本开头删除
    #[test]
    fn test_apply_delete() {
        let text = "Hello";
        let op = Operation::Delete {
            position: 0,
            length: 1,
        };
        assert_eq!(apply_operation(text, &op), "ello");
    }

    /// 测试应用 Delete 操作：从文本中间删除
    #[test]
    fn test_apply_delete_middle() {
        let text = "Hello World";
        let op = Operation::Delete {
            position: 5,
            length: 1,
        };
        assert_eq!(apply_operation(text, &op), "HelloWorld");
    }

    /// 测试应用 Delete 操作：删除全部文本
    #[test]
    fn test_apply_delete_all() {
        let text = "Hello";
        let op = Operation::Delete {
            position: 0,
            length: 5,
        };
        assert_eq!(apply_operation(text, &op), "");
    }

    // ================================================================
    // 往返一致性测试：验证 compute_operation 生成的操作用 apply_operation 能还原
    // ================================================================

    /// 辅助函数：验证计算出的操作能正确将旧文本转换为新文本
    fn verify_roundtrip(old_text: &str, new_text: &str) {
        let ops = compute_operation(old_text, new_text);
        let mut result = old_text.to_string();
        for op in &ops {
            result = apply_operation(&result, op);
        }
        assert_eq!(
            result, new_text,
            "往返一致性验证失败！\n\
             旧文本: {:?}\n\
             新文本: {:?}\n\
             操作: {:?}\n\
             实际结果: {:?}",
            old_text, new_text, ops, result
        );
    }

    /// 测试插入操作的往返一致性
    #[test]
    fn test_roundtrip_insert() {
        verify_roundtrip("abc", "abxc");
    }

    /// 测试删除操作的往返一致性
    #[test]
    fn test_roundtrip_delete() {
        verify_roundtrip("abc", "ac");
    }

    /// 测试替换操作的往返一致性
    #[test]
    fn test_roundtrip_replacement() {
        verify_roundtrip("abc", "axc");
    }

    /// 测试空文本到非空文本的往返一致性
    #[test]
    fn test_roundtrip_empty_to_text() {
        verify_roundtrip("", "hello");
    }

    /// 测试非空文本到空文本的往返一致性
    #[test]
    fn test_roundtrip_text_to_empty() {
        verify_roundtrip("hello", "");
    }

    /// 测试中文文本的往返一致性
    #[test]
    fn test_roundtrip_chinese() {
        verify_roundtrip("你好世界", "你好美丽世界");
    }

    /// 测试 emoji 文本的往返一致性
    #[test]
    fn test_roundtrip_emoji() {
        verify_roundtrip("a😀c", "a😀bc");
    }

    /// 测试无变化文本的往返一致性
    #[test]
    fn test_roundtrip_no_change() {
        verify_roundtrip("abc", "abc");
    }
}
