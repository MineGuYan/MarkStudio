//! Operational Transformation (OT) 算法模块
//!
//! 本模块实现 OT 核心算法，用于解决多人实时编辑中的文本冲突问题。
//! 支持 Insert（插入）和 Delete（删除）两种操作类型的变换与应用。
//!
//! ## 核心概念
//!
//! OT 的核心思想是：当两个并发操作发生冲突时，通过变换（transform）函数
//! 将它们转换为可以按任意顺序应用的等价操作对，保证最终一致性。
//!
//! `transform(op_a, op_b) -> (op_a', op_b')` 的含义是：
//! - `op_a'` 是 `op_a` 经过 `op_b` 影响后的等价操作，可以安全地应用在 `op_b` 之后
//! - `op_b'` 是 `op_b` 经过 `op_a` 影响后的等价操作，可以安全地应用在 `op_a` 之后
//! - 满足：apply(op_a) ∘ apply(op_b') == apply(op_b) ∘ apply(op_a')

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
        /// 插入位置（基于字符的偏移量）
        position: usize,
        /// 要插入的文本内容
        text: String,
    },
    /// 删除操作：从 `position` 开始，删除 `length` 个字符
    Delete {
        /// 删除起始位置（基于字符的偏移量）
        position: usize,
        /// 要删除的字符数量
        length: usize,
    },
}

/// 变换结果结构体，包含经过变换后的两个操作。
///
/// `(op_a', op_b')` 满足：将 `op_a` 应用到文本后再应用 `op_b'` 的结果，
/// 与将 `op_b` 应用到文本后再应用 `op_a'` 的结果完全一致。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformResult {
    /// 变换后的操作 a（可以安全地应用在 op_b 之后）
    pub op_a_prime: Operation,
    /// 变换后的操作 b（可以安全地应用在 op_a 之后）
    pub op_b_prime: Operation,
}

impl TransformResult {
    /// 创建新的变换结果
    pub fn new(op_a_prime: Operation, op_b_prime: Operation) -> Self {
        TransformResult {
            op_a_prime,
            op_b_prime,
        }
    }
}

/// 核心变换函数：处理两个并发操作之间的冲突，返回变换后的等价操作对。
///
/// 根据操作类型组合，处理以下四种冲突场景：
///
/// 1. **Insert-Insert**：两个并发插入操作
///    - 若插入位置相同，后插入者的位置需要偏移前插入者文本的长度
///    - 若位置不同，位于后面的操作位置需要偏移前面操作文本的长度
///
/// 2. **Insert-Delete**：插入操作与删除操作并发
///    - 插入位置在删除范围之前：删除操作位置需要偏移插入文本的长度
///    - 插入位置在删除范围之后：插入操作位置需要减去删除的字符数
///    - 插入位置在删除范围之内：插入操作移到删除范围起始位置，删除范围扩展
///
/// 3. **Delete-Insert**：删除操作与插入操作并发（对称于 Insert-Delete）
///
/// 4. **Delete-Delete**：两个并发删除操作
///    - 无重叠：后面的删除操作位置需要偏移
///    - 有重叠：合并重叠范围，其中一个操作变为空操作
///
/// # 参数
/// - `op_a`: 第一个并发操作
/// - `op_b`: 第二个并发操作
///
/// # 返回
/// 变换后的操作对 `(op_a', op_b')`
pub fn transform(op_a: &Operation, op_b: &Operation) -> TransformResult {
    match (op_a, op_b) {
        // ============================================================
        // 场景 1：Insert-Insert 冲突
        // 两个用户同时在不同（或相同）位置插入文本
        // ============================================================
        (
            Operation::Insert {
                position: pos_a,
                text: text_a,
            },
            Operation::Insert {
                position: pos_b,
                text: text_b,
            },
        ) => {
            if pos_a <= pos_b {
                // op_a 在 op_b 之前（或同一位置），op_a 先插入
                // op_a' 不变，op_b' 的位置需要偏移 op_a 插入文本的字符数
                TransformResult::new(
                    Operation::Insert {
                        position: *pos_a,
                        text: text_a.clone(),
                    },
                    Operation::Insert {
                        position: pos_b + char_count(text_a),
                        text: text_b.clone(),
                    },
                )
            } else {
                // op_b 在 op_a 之前，op_b 先插入
                // op_a' 的位置需要偏移 op_b 插入文本的字符数，op_b' 不变
                TransformResult::new(
                    Operation::Insert {
                        position: pos_a + char_count(text_b),
                        text: text_a.clone(),
                    },
                    Operation::Insert {
                        position: *pos_b,
                        text: text_b.clone(),
                    },
                )
            }
        }

        // ============================================================
        // 场景 2：Insert-Delete 冲突
        // 一个用户插入文本，另一个用户同时删除文本
        // ============================================================
        (
            Operation::Insert {
                position: pos_a,
                text: text_a,
            },
            Operation::Delete {
                position: pos_b,
                length: len_b,
            },
        ) => transform_insert_delete(*pos_a, text_a, *pos_b, *len_b, true),

        // ============================================================
        // 场景 3：Delete-Insert 冲突
        // 对称于场景 2，交换参数后调用同一处理函数
        // ============================================================
        (
            Operation::Delete {
                position: pos_a,
                length: len_a,
            },
            Operation::Insert {
                position: pos_b,
                text: text_b,
            },
        ) => {
            // 交换参数调用 insert-delete 处理。
            // is_insert_a=false 表示 insert 操作对应 op_b，delete 操作对应 op_a，
            // 返回值中 op_a' 对应 delete 的变换结果，op_b' 对应 insert 的变换结果，
            // 顺序已经正确，无需再交换。
            transform_insert_delete(*pos_b, text_b, *pos_a, *len_a, false)
        }

        // ============================================================
        // 场景 4：Delete-Delete 冲突
        // 两个用户同时删除文本，可能删除范围有重叠
        // ============================================================
        (
            Operation::Delete {
                position: pos_a,
                length: len_a,
            },
            Operation::Delete {
                position: pos_b,
                length: len_b,
            },
        ) => transform_delete_delete(*pos_a, *len_a, *pos_b, *len_b),
    }
}

/// 处理 Insert-Delete 冲突的内部函数。
///
/// 根据插入位置与删除范围的关系，分三种情况处理：
/// 1. 插入位置在删除范围之前（或恰好在起始位置）：删除位置后移
/// 2. 插入位置在删除范围之后：插入位置前移
/// 3. 插入位置在删除范围之内：插入移到删除起始位置，删除范围扩展
///
/// # 参数
/// - `insert_pos`: 插入位置
/// - `insert_text`: 插入的文本
/// - `delete_pos`: 删除起始位置
/// - `delete_len`: 删除长度
/// - `is_insert_a`: 标记插入操作是否为 op_a（用于构建返回值）
///
/// # 返回
/// 变换结果，其中 op_a' 和 op_b' 的顺序取决于 `is_insert_a`
fn transform_insert_delete(
    insert_pos: usize,
    insert_text: &str,
    delete_pos: usize,
    delete_len: usize,
    is_insert_a: bool,
) -> TransformResult {
    let delete_end = delete_pos + delete_len; // 删除范围的结束位置（不包含）

    if insert_pos <= delete_pos {
        // ------------------------------------------------------------
        // 情况 1：插入位置在删除范围之前（或恰好在删除起始位置）
        // 插入操作不受删除影响，但删除操作的位置需要偏移插入文本的字符数
        // ------------------------------------------------------------
        let insert_op = Operation::Insert {
            position: insert_pos,
            text: insert_text.to_string(),
        };
        let delete_op = Operation::Delete {
            position: delete_pos + char_count(insert_text),
            length: delete_len,
        };
        if is_insert_a {
            TransformResult::new(insert_op, delete_op)
        } else {
            TransformResult::new(delete_op, insert_op)
        }
    } else if insert_pos > delete_end {
        // ------------------------------------------------------------
        // 情况 2：插入位置在删除范围之后
        // 删除操作不受影响，但插入操作的位置需要减去删除的字符数
        // ------------------------------------------------------------
        let insert_op = Operation::Insert {
            position: insert_pos - delete_len,
            text: insert_text.to_string(),
        };
        let delete_op = Operation::Delete {
            position: delete_pos,
            length: delete_len,
        };
        if is_insert_a {
            TransformResult::new(insert_op, delete_op)
        } else {
            TransformResult::new(delete_op, insert_op)
        }
    } else {
        // ------------------------------------------------------------
        // 情况 3：插入位置在删除范围之内
        // 插入操作被"推"到删除范围的起始位置，删除范围扩展以包含插入的文本
        // ------------------------------------------------------------
        let insert_op = Operation::Insert {
            position: delete_pos,
            text: insert_text.to_string(),
        };
        let delete_op = Operation::Delete {
            position: delete_pos,
            length: delete_len + char_count(insert_text),
        };
        if is_insert_a {
            TransformResult::new(insert_op, delete_op)
        } else {
            TransformResult::new(delete_op, insert_op)
        }
    }
}

/// 处理 Delete-Delete 冲突的内部函数。
///
/// 根据两个删除范围是否有重叠，分两种情况处理：
/// 1. 无重叠：后面的删除操作位置需要偏移前面删除的长度
/// 2. 有重叠：保留"先出现"的删除并扩展其范围，另一个变为空操作
///
/// 重叠删除的详细处理逻辑：
/// - 假设 op_a 的删除范围起始位置 ≤ op_b 的起始位置（否则交换）
/// - op_a' 扩展为覆盖两个删除范围的并集
/// - op_b' 变为空操作（长度为 0），因为其范围内的文本已被 op_a' 删除
///
/// 通过交换参数，确保 op_a 始终是"先出现"的删除（起始位置更小），
/// 然后返回变换后的结果，再根据是否交换过还原顺序。
///
/// # 参数
/// - `pos_a`: 第一个删除的起始位置
/// - `len_a`: 第一个删除的长度
/// - `pos_b`: 第二个删除的起始位置
/// - `len_b`: 第二个删除的长度
fn transform_delete_delete(
    pos_a: usize,
    len_a: usize,
    pos_b: usize,
    len_b: usize,
) -> TransformResult {
    let end_a = pos_a + len_a; // op_a 删除范围的结束位置（不包含）
    let end_b = pos_b + len_b; // op_b 删除范围的结束位置（不包含）

    if end_a <= pos_b {
        // ------------------------------------------------------------
        // 情况 1：op_a 完全在 op_b 之前，无重叠
        // op_a' 不变，op_b' 的位置需要减去 op_a 删除的长度
        // ------------------------------------------------------------
        TransformResult::new(
            Operation::Delete {
                position: pos_a,
                length: len_a,
            },
            Operation::Delete {
                position: pos_b - len_a,
                length: len_b,
            },
        )
    } else if end_b <= pos_a {
        // ------------------------------------------------------------
        // 情况 2：op_b 完全在 op_a 之前，无重叠
        // op_a' 的位置需要减去 op_b 删除的长度，op_b' 不变
        // ------------------------------------------------------------
        TransformResult::new(
            Operation::Delete {
                position: pos_a - len_b,
                length: len_a,
            },
            Operation::Delete {
                position: pos_b,
                length: len_b,
            },
        )
    } else {
        // ------------------------------------------------------------
        // 情况 3：两个删除范围有重叠
        // 需要精确计算各自在对方操作之后仍需要删除的存活范围。
        //
        // 核心思路：对于 op_a'（应用在 op_b 之后），
        //   需要删除的范围 = op_a 的原始范围减去被 op_b 已经删除的部分。
        //   即：[a_start, a_end) \ [b_start, b_end)
        //   = [a_start, min(a_end, b_start)) ∪ [max(a_start, b_end), a_end)
        //
        // 在 op_b 执行后，第一段位置不变，第二段会左移 len_b 个单位。
        // 如果两段都存在（即 op_a 包含 op_b），它们恰好是连续的。
        // ------------------------------------------------------------

        // 计算 op_a'：op_a 在 op_b 之后需要删除的范围
        let (a_prime_pos, a_prime_len) = surviving_delete_range(pos_a, end_a, pos_b, end_b);

        // 计算 op_b'：op_b 在 op_a 之后需要删除的范围
        let (b_prime_pos, b_prime_len) = surviving_delete_range(pos_b, end_b, pos_a, end_a);

        TransformResult::new(
            Operation::Delete {
                position: a_prime_pos,
                length: a_prime_len,
            },
            Operation::Delete {
                position: b_prime_pos,
                length: b_prime_len,
            },
        )
    }
}

/// 计算一个删除操作在另一个删除操作执行后，仍需删除的存活范围。
///
/// 给定原始删除范围 [my_start, my_end) 和先执行的删除范围 [other_start, other_end)，
/// 返回在 other 删除执行后，my 仍需删除的位置和长度。
///
/// 存活范围 = [my_start, my_end) \ [other_start, other_end)
///   = [my_start, min(my_end, other_start)) ∪ [max(my_start, other_end), my_end)
///
/// 在 other 删除之后：
/// - 第一段 [my_start, min(my_end, other_start)) 位置不变
/// - 第二段 [max(my_start, other_end), my_end) 左移 other_len 个单位到
///   [other_start, other_start + my_end - max(my_start, other_end))
///
/// 如果两段都存在（my 包含 other），它们恰好是连续的，可以合并为一段。
fn surviving_delete_range(
    my_start: usize,
    my_end: usize,
    other_start: usize,
    other_end: usize,
) -> (usize, usize) {
    // 计算第一段存活范围：[my_start, min(my_end, other_start))
    let first_end = my_end.min(other_start);
    let first_len = first_end.saturating_sub(my_start);

    // 计算第二段存活范围：[max(my_start, other_end), my_end)
    let second_start = my_start.max(other_end);
    let second_len = my_end.saturating_sub(second_start);

    if first_len > 0 && second_len > 0 {
        // 两段都存在：my 的删除范围完全包含了 other 的删除范围
        // 合并为一段连续范围：位置 = my_start，长度 = first_len + second_len
        (my_start, first_len + second_len)
    } else if first_len > 0 {
        // 只有第一段存活：my 的删除范围完全在 other 之前
        // 位置不变
        (my_start, first_len)
    } else if second_len > 0 {
        // 只有第二段存活：my 的删除范围部分在 other 之后
        // 第二段左移 other_len 到 other_start 位置
        (other_start, second_len)
    } else {
        // 没有存活范围：my 的删除范围完全被 other 覆盖
        // 返回空操作（位置为 0，长度为 0）
        (0, 0)
    }
}

/// 计算字符串的字符数（Unicode 标量值数量），而非字节长度。
///
/// 对于多字节 UTF-8 字符（如中文），字符数小于字节长度。
/// 例如："啊" 的字节长度为 3，但字符数为 1。
#[inline]
fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// 将基于字符的偏移量转换为基于字节的偏移量，用于安全的字符串切片。
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
/// # 参数
/// - `text`: 原始文本
/// - `op`: 要应用的操作
///
/// # 返回
/// 应用操作后的新文本
///
/// # 示例
/// ```
/// let text = "Hello World";
/// let op = Operation::Insert { position: 6, text: "Beautiful ".to_string() };
/// assert_eq!(apply_operation(text, &op), "Hello Beautiful World");
/// ```
pub fn apply_operation(text: &str, op: &Operation) -> String {
    match op {
        Operation::Insert {
            position,
            text: insert_text,
        } => {
            // 插入操作：在指定字符位置之前插入文本
            // 需要将原始文本分割为前后两部分，中间插入新文本
            let text_char_count = char_count(text);
            if *position >= text_char_count {
                // 插入位置在文本末尾或之后，直接追加
                let mut result = text.to_string();
                result.push_str(insert_text);
                result
            } else {
                // 在文本中间插入
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

/// 批量应用操作序列到文本上，依次执行每个操作。
///
/// 这是 `apply_operation` 的便捷封装，用于将一整个操作序列按顺序
/// 应用到初始文本上，生成最终文本。
///
/// # 参数
/// - `ops`: 操作切片，按顺序应用
///
/// # 返回
/// 应用所有操作后的最终文本
///
/// # 示例
/// ```
/// let ops = vec![
///     Operation::Insert { position: 0, text: "Hello".to_string() },
///     Operation::Insert { position: 5, text: " World".to_string() },
/// ];
/// assert_eq!(compose_operations(&ops), "Hello World");
/// ```
pub fn compose_operations(ops: &[Operation]) -> String {
    let mut text = String::new();
    for op in ops {
        text = apply_operation(&text, op);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    // ================================================================
    // apply_operation 测试
    // ================================================================

    #[test]
    fn test_apply_insert_at_beginning() {
        let text = "World";
        let op = Operation::Insert {
            position: 0,
            text: "Hello ".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hello World");
    }

    #[test]
    fn test_apply_insert_at_end() {
        let text = "Hello";
        let op = Operation::Insert {
            position: 5,
            text: " World".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hello World");
    }

    #[test]
    fn test_apply_insert_in_middle() {
        let text = "HelloWorld";
        let op = Operation::Insert {
            position: 5,
            text: " ".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hello World");
    }

    #[test]
    fn test_apply_insert_empty_text() {
        let text = "";
        let op = Operation::Insert {
            position: 0,
            text: "Hello".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hello");
    }

    #[test]
    fn test_apply_insert_beyond_length() {
        // 插入位置超出文本长度时，应直接追加到末尾
        let text = "Hi";
        let op = Operation::Insert {
            position: 100,
            text: "!".to_string(),
        };
        assert_eq!(apply_operation(text, &op), "Hi!");
    }

    #[test]
    fn test_apply_delete_at_beginning() {
        let text = "Hello World";
        let op = Operation::Delete {
            position: 0,
            length: 6,
        };
        assert_eq!(apply_operation(text, &op), "World");
    }

    #[test]
    fn test_apply_delete_at_end() {
        let text = "Hello World";
        let op = Operation::Delete {
            position: 5,
            length: 6,
        };
        assert_eq!(apply_operation(text, &op), "Hello");
    }

    #[test]
    fn test_apply_delete_in_middle() {
        let text = "Hello Beautiful World";
        let op = Operation::Delete {
            position: 6,
            length: 10,
        };
        assert_eq!(apply_operation(text, &op), "Hello World");
    }

    #[test]
    fn test_apply_delete_partial_length() {
        // 删除长度超出文本范围时，只删除到文本末尾
        let text = "Hi";
        let op = Operation::Delete {
            position: 1,
            length: 100,
        };
        assert_eq!(apply_operation(text, &op), "H");
    }

    #[test]
    fn test_apply_delete_beyond_text() {
        // 删除起始位置超出文本范围，应返回原文本
        let text = "Hi";
        let op = Operation::Delete {
            position: 100,
            length: 5,
        };
        assert_eq!(apply_operation(text, &op), "Hi");
    }

    #[test]
    fn test_apply_delete_all() {
        let text = "Hello";
        let op = Operation::Delete {
            position: 0,
            length: 5,
        };
        assert_eq!(apply_operation(text, &op), "");
    }

    #[test]
    fn test_apply_delete_empty_text() {
        let text = "";
        let op = Operation::Delete {
            position: 0,
            length: 5,
        };
        assert_eq!(apply_operation(text, &op), "");
    }

    // ================================================================
    // compose_operations 测试
    // ================================================================

    #[test]
    fn test_compose_empty_ops() {
        let ops: Vec<Operation> = vec![];
        assert_eq!(compose_operations(&ops), "");
    }

    #[test]
    fn test_compose_multiple_inserts() {
        let ops = vec![
            Operation::Insert {
                position: 0,
                text: "Hello".to_string(),
            },
            Operation::Insert {
                position: 5,
                text: " ".to_string(),
            },
            Operation::Insert {
                position: 6,
                text: "World".to_string(),
            },
        ];
        assert_eq!(compose_operations(&ops), "Hello World");
    }

    #[test]
    fn test_compose_insert_and_delete() {
        let ops = vec![
            Operation::Insert {
                position: 0,
                text: "Hello Beautiful World".to_string(),
            },
            Operation::Delete {
                position: 6,
                length: 10,
            },
        ];
        assert_eq!(compose_operations(&ops), "Hello World");
    }

    #[test]
    fn test_compose_delete_and_insert() {
        let ops = vec![
            Operation::Insert {
                position: 0,
                text: "Hello World".to_string(),
            },
            Operation::Delete {
                position: 5,
                length: 1,
            },
            Operation::Insert {
                position: 5,
                text: ",".to_string(),
            },
        ];
        assert_eq!(compose_operations(&ops), "Hello,World");
    }

    // ================================================================
    // transform Insert-Insert 测试
    // ================================================================

    #[test]
    fn test_transform_insert_insert_same_position() {
        // 两个用户同时在位置 5 插入不同文本
        // op_a 插入 "abc"，op_b 插入 "xyz"
        let op_a = Operation::Insert {
            position: 5,
            text: "abc".to_string(),
        };
        let op_b = Operation::Insert {
            position: 5,
            text: "xyz".to_string(),
        };

        let result = transform(&op_a, &op_b);

        // op_a' 不变（仍插入在位置 5）
        assert_eq!(
            result.op_a_prime,
            Operation::Insert {
                position: 5,
                text: "abc".to_string(),
            }
        );
        // op_b' 位置偏移 3（op_a 文本长度）
        assert_eq!(
            result.op_b_prime,
            Operation::Insert {
                position: 8,
                text: "xyz".to_string(),
            }
        );
    }

    #[test]
    fn test_transform_insert_insert_different_position_a_before_b() {
        // op_a 在位置 5，op_b 在位置 10
        let op_a = Operation::Insert {
            position: 5,
            text: "abc".to_string(),
        };
        let op_b = Operation::Insert {
            position: 10,
            text: "xyz".to_string(),
        };

        let result = transform(&op_a, &op_b);

        // op_a' 不变
        assert_eq!(
            result.op_a_prime,
            Operation::Insert {
                position: 5,
                text: "abc".to_string(),
            }
        );
        // op_b' 位置偏移 3
        assert_eq!(
            result.op_b_prime,
            Operation::Insert {
                position: 13,
                text: "xyz".to_string(),
            }
        );
    }

    #[test]
    fn test_transform_insert_insert_different_position_b_before_a() {
        // op_b 在位置 5，op_a 在位置 10
        let op_a = Operation::Insert {
            position: 10,
            text: "abc".to_string(),
        };
        let op_b = Operation::Insert {
            position: 5,
            text: "xyz".to_string(),
        };

        let result = transform(&op_a, &op_b);

        // op_a' 位置偏移 3
        assert_eq!(
            result.op_a_prime,
            Operation::Insert {
                position: 13,
                text: "abc".to_string(),
            }
        );
        // op_b' 不变
        assert_eq!(
            result.op_b_prime,
            Operation::Insert {
                position: 5,
                text: "xyz".to_string(),
            }
        );
    }

    // ================================================================
    // transform Insert-Delete 测试
    // ================================================================

    #[test]
    fn test_transform_insert_before_delete() {
        // 插入位置在删除范围之前
        let op_a = Operation::Insert {
            position: 3,
            text: "abc".to_string(),
        };
        let op_b = Operation::Delete {
            position: 5,
            length: 3,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 不变：插入在删除之前，不受影响
        assert_eq!(
            result.op_a_prime,
            Operation::Insert {
                position: 3,
                text: "abc".to_string(),
            }
        );
        // op_b' 位置后移 3：删除范围被插入文本推后
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 8,
                length: 3,
            }
        );
    }

    #[test]
    fn test_transform_insert_at_delete_start() {
        // 插入位置恰好在删除起始位置
        let op_a = Operation::Insert {
            position: 5,
            text: "abc".to_string(),
        };
        let op_b = Operation::Delete {
            position: 5,
            length: 3,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 不变
        assert_eq!(
            result.op_a_prime,
            Operation::Insert {
                position: 5,
                text: "abc".to_string(),
            }
        );
        // op_b' 位置后移
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 8,
                length: 3,
            }
        );
    }

    #[test]
    fn test_transform_insert_after_delete() {
        // 插入位置在删除范围之后
        let op_a = Operation::Insert {
            position: 10,
            text: "abc".to_string(),
        };
        let op_b = Operation::Delete {
            position: 3,
            length: 3,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 位置前移 3
        assert_eq!(
            result.op_a_prime,
            Operation::Insert {
                position: 7,
                text: "abc".to_string(),
            }
        );
        // op_b' 不变
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 3,
                length: 3,
            }
        );
    }

    #[test]
    fn test_transform_insert_inside_delete() {
        // 插入位置在删除范围之内
        let op_a = Operation::Insert {
            position: 6,
            text: "abc".to_string(),
        };
        let op_b = Operation::Delete {
            position: 5,
            length: 5,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 被推到删除起始位置
        assert_eq!(
            result.op_a_prime,
            Operation::Insert {
                position: 5,
                text: "abc".to_string(),
            }
        );
        // op_b' 删除范围扩展
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 5,
                length: 8,
            }
        );
    }

    // ================================================================
    // transform Delete-Insert 测试
    // ================================================================

    #[test]
    fn test_transform_delete_before_insert() {
        // 删除范围在插入位置之前
        let op_a = Operation::Delete {
            position: 3,
            length: 3,
        };
        let op_b = Operation::Insert {
            position: 10,
            text: "abc".to_string(),
        };

        let result = transform(&op_a, &op_b);

        // op_a' 不变
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 3,
                length: 3,
            }
        );
        // op_b' 位置前移 3
        assert_eq!(
            result.op_b_prime,
            Operation::Insert {
                position: 7,
                text: "abc".to_string(),
            }
        );
    }

    #[test]
    fn test_transform_delete_after_insert() {
        // 删除范围在插入位置之后
        let op_a = Operation::Delete {
            position: 8,
            length: 3,
        };
        let op_b = Operation::Insert {
            position: 3,
            text: "abc".to_string(),
        };

        let result = transform(&op_a, &op_b);

        // op_a' 位置后移 3
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 11,
                length: 3,
            }
        );
        // op_b' 不变
        assert_eq!(
            result.op_b_prime,
            Operation::Insert {
                position: 3,
                text: "abc".to_string(),
            }
        );
    }

    #[test]
    fn test_transform_delete_contains_insert() {
        // 删除范围包含插入位置
        let op_a = Operation::Delete {
            position: 5,
            length: 5,
        };
        let op_b = Operation::Insert {
            position: 7,
            text: "abc".to_string(),
        };

        let result = transform(&op_a, &op_b);

        // op_a' 删除范围扩展
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 5,
                length: 8,
            }
        );
        // op_b' 被推到删除起始位置
        assert_eq!(
            result.op_b_prime,
            Operation::Insert {
                position: 5,
                text: "abc".to_string(),
            }
        );
    }

    // ================================================================
    // transform Delete-Delete 测试
    // ================================================================

    #[test]
    fn test_transform_delete_delete_no_overlap_a_before_b() {
        // op_a 完全在 op_b 之前，无重叠
        let op_a = Operation::Delete {
            position: 3,
            length: 3,
        };
        let op_b = Operation::Delete {
            position: 10,
            length: 3,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 不变
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 3,
                length: 3,
            }
        );
        // op_b' 位置前移 3
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 7,
                length: 3,
            }
        );
    }

    #[test]
    fn test_transform_delete_delete_no_overlap_b_before_a() {
        // op_b 完全在 op_a 之前，无重叠
        let op_a = Operation::Delete {
            position: 10,
            length: 3,
        };
        let op_b = Operation::Delete {
            position: 3,
            length: 3,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 位置前移 3
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 7,
                length: 3,
            }
        );
        // op_b' 不变
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 3,
                length: 3,
            }
        );
    }

    #[test]
    fn test_transform_delete_delete_adjacent() {
        // 两个删除紧邻（op_a 结束位置 == op_b 起始位置），无重叠
        let op_a = Operation::Delete {
            position: 3,
            length: 3,
        }; // 删除 [3, 6)
        let op_b = Operation::Delete {
            position: 6,
            length: 3,
        }; // 删除 [6, 9)

        let result = transform(&op_a, &op_b);

        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 3,
                length: 3,
            }
        );
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 3,
                length: 3,
            }
        );
    }

    #[test]
    fn test_transform_delete_delete_partial_overlap() {
        // 两个删除部分重叠
        // op_a 删除 [5, 10)，op_b 删除 [8, 13)
        let op_a = Operation::Delete {
            position: 5,
            length: 5,
        };
        let op_b = Operation::Delete {
            position: 8,
            length: 5,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 删除存活部分 [5, 8)，位置不变，长度 3
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 5,
                length: 3,
            }
        );
        // op_b' 删除存活部分 [10, 13)，左移 5 到 [5, 8)，长度 3
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 5,
                length: 3,
            }
        );
    }

    #[test]
    fn test_transform_delete_delete_a_contains_b() {
        // op_a 完全包含 op_b：op_a 删除 [5, 15)，op_b 删除 [8, 11)
        let op_a = Operation::Delete {
            position: 5,
            length: 10,
        };
        let op_b = Operation::Delete {
            position: 8,
            length: 3,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 保留不在 op_b 范围内的部分：
        // [5, 8) 位置不变 + [11, 15) 左移 3 到 [8, 12)，合并为 [5, 12)，长度 7
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 5,
                length: 7,
            }
        );
        // op_b' 完全被 op_a 覆盖，变为空操作
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 0,
                length: 0,
            }
        );
    }

    #[test]
    fn test_transform_delete_delete_b_contains_a() {
        // op_b 完全包含 op_a：op_a 删除 [8, 11)，op_b 删除 [5, 15)
        let op_a = Operation::Delete {
            position: 8,
            length: 3,
        };
        let op_b = Operation::Delete {
            position: 5,
            length: 10,
        };

        let result = transform(&op_a, &op_b);

        // op_a' 完全被 op_b 覆盖，变为空操作
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 0,
                length: 0,
            }
        );
        // op_b' 保留不在 op_a 范围内的部分：
        // [5, 8) 位置不变 + [11, 15) 左移 3 到 [8, 12)，合并为 [5, 12)，长度 7
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 5,
                length: 7,
            }
        );
    }

    #[test]
    fn test_transform_delete_delete_same_range() {
        // 两个删除范围完全相同，相互完全覆盖
        let op_a = Operation::Delete {
            position: 5,
            length: 5,
        };
        let op_b = Operation::Delete {
            position: 5,
            length: 5,
        };

        let result = transform(&op_a, &op_b);

        // 两者互相完全覆盖，都变为空操作
        assert_eq!(
            result.op_a_prime,
            Operation::Delete {
                position: 0,
                length: 0,
            }
        );
        assert_eq!(
            result.op_b_prime,
            Operation::Delete {
                position: 0,
                length: 0,
            }
        );
    }

    // ================================================================
    // 变换一致性验证测试
    // 验证核心属性：apply(op_a) ∘ apply(op_b') == apply(op_b) ∘ apply(op_a')
    // ================================================================

    /// 辅助函数：验证变换的一致性
    /// 即：先应用 op_a 再应用 op_b' 的结果，必须等于先应用 op_b 再应用 op_a' 的结果
    fn verify_transform_consistency(initial_text: &str, op_a: &Operation, op_b: &Operation) {
        let result = transform(op_a, op_b);

        // 路径 1：先应用 op_a，再应用 op_b'
        let after_a = apply_operation(initial_text, op_a);
        let after_a_then_b = apply_operation(&after_a, &result.op_b_prime);

        // 路径 2：先应用 op_b，再应用 op_a'
        let after_b = apply_operation(initial_text, op_b);
        let after_b_then_a = apply_operation(&after_b, &result.op_a_prime);

        assert_eq!(
            after_a_then_b, after_b_then_a,
            "变换一致性验证失败！\n\
             初始文本: {:?}\n\
             op_a: {:?}\n\
             op_b: {:?}\n\
             路径1 (a→b'): {:?}\n\
             路径2 (b→a'): {:?}",
            initial_text, op_a, op_b, after_a_then_b, after_b_then_a
        );
    }

    #[test]
    fn test_consistency_insert_insert_same_pos() {
        let initial = "Hello World";
        let op_a = Operation::Insert {
            position: 5,
            text: "AAA".to_string(),
        };
        let op_b = Operation::Insert {
            position: 5,
            text: "BBB".to_string(),
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    #[test]
    fn test_consistency_insert_insert_different_pos() {
        let initial = "Hello World";
        let op_a = Operation::Insert {
            position: 2,
            text: "AAA".to_string(),
        };
        let op_b = Operation::Insert {
            position: 8,
            text: "BBB".to_string(),
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    #[test]
    fn test_consistency_insert_delete() {
        let initial = "Hello Beautiful World";
        let op_a = Operation::Insert {
            position: 6,
            text: "XXX".to_string(),
        };
        let op_b = Operation::Delete {
            position: 6,
            length: 10,
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    #[test]
    fn test_consistency_delete_insert() {
        let initial = "Hello Beautiful World";
        let op_a = Operation::Delete {
            position: 6,
            length: 10,
        };
        let op_b = Operation::Insert {
            position: 6,
            text: "XXX".to_string(),
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    #[test]
    fn test_consistency_delete_delete_no_overlap() {
        let initial = "Hello Beautiful World";
        let op_a = Operation::Delete {
            position: 3,
            length: 2,
        };
        let op_b = Operation::Delete {
            position: 10,
            length: 3,
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    #[test]
    fn test_consistency_delete_delete_overlap() {
        let initial = "Hello Beautiful World";
        let op_a = Operation::Delete {
            position: 6,
            length: 6,
        };
        let op_b = Operation::Delete {
            position: 8,
            length: 6,
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    #[test]
    fn test_consistency_empty_text() {
        let initial = "";
        let op_a = Operation::Insert {
            position: 0,
            text: "Hello".to_string(),
        };
        let op_b = Operation::Insert {
            position: 0,
            text: "World".to_string(),
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    #[test]
    fn test_consistency_at_text_boundaries() {
        let initial = "Hello";
        // 在文本开头和结尾同时操作
        let op_a = Operation::Insert {
            position: 0,
            text: "AAA".to_string(),
        };
        let op_b = Operation::Delete {
            position: 4,
            length: 1,
        };
        verify_transform_consistency(initial, &op_a, &op_b);
    }

    // ================================================================
    // 序列化测试
    // ================================================================

    #[test]
    fn test_serialize_operation_insert() {
        let op = Operation::Insert {
            position: 5,
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Operation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_serialize_operation_delete() {
        let op = Operation::Delete {
            position: 5,
            length: 3,
        };
        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Operation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_serialize_transform_result() {
        let result = TransformResult::new(
            Operation::Insert {
                position: 5,
                text: "abc".to_string(),
            },
            Operation::Delete {
                position: 3,
                length: 2,
            },
        );
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TransformResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }
}
