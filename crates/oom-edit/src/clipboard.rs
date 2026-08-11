//! OSC 52 system clipboard implementation.
//!
//! [`Osc52Clipboard`] implements the core [`ClipboardSink`] trait by emitting
//! the OSC 52 escape sequence `ESC ] 5 2 ; c ; <base64(text)> BEL` to stdout.
//!
//! Payloads >100 KB are rejected with `ClipboardError::TooLarge` (OSC 52
//! practical limit).
//!
//! See plan V-R3, risk 7.

use std::io::{self, Write};
#[cfg(test)]
use std::sync::{Arc, Mutex};

use oom_edit_core::{ClipboardError, ClipboardSink};

/// Maximum payload size for OSC 52 clipboard writes (100 KB).
const MAX_PAYLOAD: usize = 100 * 1024;

/// OSC 52 escape sequence delimiters.
const OSC_START: &str = "\x1b]52;c;";
const OSC_END: &str = "\x07"; // BEL

/// A clipboard sink that writes via OSC 52 escape sequences to stdout.
///
/// Emits `ESC ] 5 2 ; c ; <base64(text)> BEL` for each [`ClipboardSink::copy`]
/// call. Payloads exceeding 100 KB are rejected.
pub struct Osc52Clipboard {
    /// The writer to emit escape sequences to (typically stdout).
    writer: Box<dyn Write + Send>,
}

impl Osc52Clipboard {
    /// Create a new `Osc52Clipboard` that writes to the given writer.
    #[allow(dead_code)]
    pub fn new<W: Write + Send + 'static>(writer: W) -> Self {
        Self {
            writer: Box::new(writer),
        }
    }

    /// Create a new `Osc52Clipboard` that writes to stdout.
    pub fn stdout() -> Self {
        Self {
            writer: Box::new(io::stdout()),
        }
    }

    /// Create a new `Osc52Clipboard` that writes to a test buffer (for tests).
    /// Returns the sink and a `CaptureWriter` that can be inspected after the
    /// sink is dropped.
    #[cfg(test)]
    pub(crate) fn for_test() -> (Self, CaptureWriter) {
        let capture = CaptureWriter::new();
        let sink = Self {
            writer: Box::new(capture.clone()),
        };
        (sink, capture)
    }
}

/// A thread-safe writer that captures output for tests.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct CaptureWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

#[cfg(test)]
impl CaptureWriter {
    pub(crate) fn new() -> Self {
        Self {
            buf: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn contents(&self) -> Vec<u8> {
        self.buf.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ClipboardSink for Osc52Clipboard {
    fn copy(&mut self, text: &str) -> Result<(), ClipboardError> {
        if text.len() > MAX_PAYLOAD {
            return Err(ClipboardError::TooLarge);
        }

        let encoded = base64_encode(text);
        let sequence = format!("{}{}{}", OSC_START, encoded, OSC_END);

        // Write the OSC 52 sequence to the terminal.
        // If the write fails, the terminal likely doesn't support OSC 52.
        self.writer
            .write_all(sequence.as_bytes())
            .map_err(|e| ClipboardError::Other(e.to_string()))?;

        self.writer
            .flush()
            .map_err(|e| ClipboardError::Other(e.to_string()))?;

        Ok(())
    }
}

/// Hand-rolled base64 encoding per RFC 4648 (no external dependency).
///
/// Uses the standard alphabet: A-Z, a-z, 0-9, +, /.
/// Output is unpadded (trailing `=` removed) to save bytes — terminals
/// that decode OSC 52 payloads handle both padded and unpadded.
fn base64_encode(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut output = Vec::with_capacity(bytes.len().div_ceil(3) * 4);

    let mut i = 0;
    while i < bytes.len() {
        // Read up to 3 bytes
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() {
            bytes[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < bytes.len() {
            bytes[i + 2] as u32
        } else {
            0
        };

        // Pack into 24 bits
        let triple = (b0 << 16) | (b1 << 8) | b2;

        // Emit 4 base64 characters
        output.push(ALPHABET[((triple >> 18) & 0x3F) as usize]);
        output.push(ALPHABET[((triple >> 12) & 0x3F) as usize]);

        if i + 1 < bytes.len() {
            output.push(ALPHABET[((triple >> 6) & 0x3F) as usize]);
        }
        if i + 2 < bytes.len() {
            output.push(ALPHABET[(triple & 0x3F) as usize]);
        }

        i += 3;
    }

    // Unpadded output — strip trailing '=' characters
    let result = String::from_utf8(output).unwrap();
    result.trim_end_matches('=').to_string()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648 §10 test vector: "f" → "Zg" (unpadded).
    #[test]
    fn base64_encode_single_byte() {
        assert_eq!(base64_encode("f"), "Zg");
    }

    /// RFC 4648 §10 test vector: "fo" → "Zm8" (unpadded).
    #[test]
    fn base64_encode_two_bytes() {
        assert_eq!(base64_encode("fo"), "Zm8");
    }

    /// RFC 4648 §10 test vector: "foo" → "Zm9v" (unpadded).
    #[test]
    fn base64_encode_three_bytes() {
        assert_eq!(base64_encode("foo"), "Zm9v");
    }

    /// RFC 4648 §10 test vector: "foob" → "Zm9vYg" (unpadded).
    #[test]
    fn base64_encode_four_bytes() {
        assert_eq!(base64_encode("foob"), "Zm9vYg");
    }

    /// RFC 4648 §10 test vector: "fooba" → "Zm9vYmE" (unpadded).
    #[test]
    fn base64_encode_five_bytes() {
        assert_eq!(base64_encode("fooba"), "Zm9vYmE");
    }

    /// RFC 4648 §10 test vector: "foobar" → "Zm9vYmFy" (unpadded).
    #[test]
    fn base64_encode_six_bytes() {
        assert_eq!(base64_encode("foobar"), "Zm9vYmFy");
    }

    /// Empty string encodes to empty string.
    #[test]
    fn base64_encode_empty() {
        assert_eq!(base64_encode(""), "");
    }

    /// Multi-line text with special characters.
    #[test]
    fn base64_encode_multiline() {
        // "hello\nworld" → "aGVsbG8Kd29ybGQ=" → unpadded "aGVsbG8Kd29ybGQ"
        assert_eq!(base64_encode("hello\nworld"), "aGVsbG8Kd29ybGQ");
    }

    /// Unicode text (UTF-8 bytes).
    #[test]
    fn base64_encode_unicode() {
        // "café" in UTF-8 is [99, 97, 102, 195, 169] — unpadded output
        assert_eq!(base64_encode("café"), "Y2Fmw6k");
    }

    /// OSC 52 sequence format is correct.
    #[test]
    fn osc52_sequence_format() {
        let (mut sink, capture) = Osc52Clipboard::for_test();
        sink.copy("test").unwrap();
        let expected = format!("{}{}{}", OSC_START, base64_encode("test"), OSC_END);
        assert_eq!(String::from_utf8(capture.contents()).unwrap(), expected);
    }

    /// Oversized payload is rejected.
    #[test]
    fn osc52_rejects_oversized() {
        let (mut sink, capture) = Osc52Clipboard::for_test();
        let large = "x".repeat(MAX_PAYLOAD + 1);
        assert_eq!(sink.copy(&large), Err(ClipboardError::TooLarge));
        assert!(capture.contents().is_empty());
    }
}
