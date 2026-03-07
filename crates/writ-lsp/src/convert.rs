use writ::lexer::Span;

/// Converts a Writ `Span` (1-indexed) to an LSP `Range` (0-indexed).
pub fn span_to_range(span: &Span) -> lsp_types::Range {
    let start = lsp_types::Position {
        line: span.line.saturating_sub(1),
        character: span.column.saturating_sub(1),
    };
    let end = lsp_types::Position {
        line: span.line.saturating_sub(1),
        character: span.column.saturating_sub(1) + span.length,
    };
    lsp_types::Range { start, end }
}

/// Converts an LSP `Position` (0-indexed) to a byte offset in the source string.
pub fn position_to_offset(source: &str, pos: lsp_types::Position) -> Option<usize> {
    let mut offset = 0;
    for (i, line) in source.split('\n').enumerate() {
        if i == pos.line as usize {
            let char_offset = pos.character as usize;
            if char_offset <= line.len() {
                return Some(offset + char_offset);
            } else {
                return None;
            }
        }
        offset += line.len() + 1; // +1 for '\n'
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Converts a byte offset to an LSP `Position` (0-indexed).
    fn offset_to_position(source: &str, offset: usize) -> lsp_types::Position {
        let mut line = 0u32;
        let mut col = 0u32;
        for (i, ch) in source.char_indices() {
            if i == offset {
                return lsp_types::Position {
                    line,
                    character: col,
                };
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        lsp_types::Position {
            line,
            character: col,
        }
    }

    #[test]
    fn span_to_range_converts_1_indexed_to_0_indexed() {
        let span = Span {
            file: String::new(),
            line: 1,
            column: 1,
            length: 3,
        };
        let range = span_to_range(&span);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.character, 3);
    }

    #[test]
    fn position_to_offset_first_line() {
        let source = "let x = 42";
        let pos = lsp_types::Position {
            line: 0,
            character: 4,
        };
        assert_eq!(position_to_offset(source, pos), Some(4));
    }

    #[test]
    fn position_to_offset_second_line() {
        let source = "let x = 42\nlet y = 10";
        let pos = lsp_types::Position {
            line: 1,
            character: 4,
        };
        assert_eq!(position_to_offset(source, pos), Some(15));
    }

    #[test]
    fn position_to_offset_out_of_bounds() {
        let source = "abc";
        let pos = lsp_types::Position {
            line: 0,
            character: 10,
        };
        assert_eq!(position_to_offset(source, pos), None);
    }

    #[test]
    fn offset_to_position_roundtrip() {
        let source = "hello\nworld";
        let pos = offset_to_position(source, 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }
}
