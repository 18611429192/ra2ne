//! Explicit legacy text decoding; never silently replace invalid byte sequences.
use std::borrow::Cow;
#[derive(Clone, Copy, Debug)]
pub enum TextEncoding {
    Utf8,
    Windows1252,
    Gbk,
}
impl TextEncoding {
    pub fn parse(name: &str) -> Result<Self, &'static str> {
        match name.to_ascii_lowercase().as_str() {
            "utf8" | "utf-8" => Ok(Self::Utf8),
            "windows1252" | "windows-1252" => Ok(Self::Windows1252),
            "gbk" | "cp936" => Ok(Self::Gbk),
            _ => Err("encoding must be utf8, windows1252 or gbk"),
        }
    }
    pub fn decode(self, bytes: &[u8]) -> Result<Cow<'_, str>, &'static str> {
        if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
            return std::str::from_utf8(bytes)
                .map(Cow::Borrowed)
                .map_err(|_| "invalid UTF-8 BOM text");
        }
        let codec = match self {
            Self::Utf8 => {
                return std::str::from_utf8(bytes)
                    .map(Cow::Borrowed)
                    .map_err(|_| "invalid UTF-8: select explicit legacy encoding");
            }
            Self::Windows1252 => encoding_rs::WINDOWS_1252,
            Self::Gbk => encoding_rs::GBK,
        };
        let (text, errors) = codec.decode_without_bom_handling(bytes);
        if errors {
            Err("invalid legacy text sequence")
        } else {
            Ok(text)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_decoding_preserves_chinese_and_rejects_invalid_input() {
        assert_eq!(
            TextEncoding::Gbk.decode(&[0xd6, 0xd0, 0xce, 0xc4]).unwrap(),
            "中文"
        );
        assert_eq!(TextEncoding::Windows1252.decode(&[0xe9]).unwrap(), "é");
        assert_eq!(
            TextEncoding::Gbk.decode(&[0xef, 0xbb, 0xbf, b'a']).unwrap(),
            "a"
        );
        assert!(TextEncoding::Utf8.decode(&[0xd6]).is_err());
        assert!(TextEncoding::Gbk.decode(&[0x81]).is_err());
        assert!(TextEncoding::parse("unknown").is_err());
    }
}
