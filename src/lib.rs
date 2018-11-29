/// This crate provides a reader that detects and transcodes from arbitrary text encodings into
/// utf-8 as an internal representation on which xml parsers can operate.
///
/// It also performs end-of-line normalization as required by [xml 1.0 specification's section
/// 2.11 -- End-of-Line Handling](https://www.w3.org/TR/xml/#sec-line-ends).
///
/// If no encoding name is provided on initialization, the reader makes some assumptions about the
/// input in order to try to detect the input encoding.  See [xml 1.0 specification's section F --
/// Autodetection of Character Encodings(Non-Normative)](https://www.w3.org/TR/xml/#sec-guessing).
///
/// Generally speaking, it checks first for a BOM, and if present treats the data as the encoding
/// corresponding to that BOM.  If no BOM is present, it relies on the fact that the xml
/// declaration (if present) must consist of ascii characters, so it checks the byte order to
/// determine how to parse the xml declaration far enough to see the encoding declaration.
///
/// It is an error for a document to be in a non-UTF/UCS encoding and lack an encoding declaration.
extern crate encoding;
use encoding::types::EncodingRef;

use std::cmp;
use std::fmt;
use std::io;
use std::io::BufRead;
use std::io::Read;

mod enc_detect;
use enc_detect::detect_encoding_with_suggestion;

const DEFAULT_BUF_SIZE: usize = 4096;

pub struct XmlReadBuffer<R> {
    inner: R,
    decoder: EncodingRef,
    input_buf: Vec<u8>,
    output_buf: String,
    output_pos: usize,
}

impl<R: Read> XmlReadBuffer<R> {
    /// Trivial constructor using default value for size of the read buffer,
    /// as well as automatically detecting the input encoding.
    pub fn new(inner: R) -> io::Result<Self> {
        Self::with_capacity_and_input_encoding(DEFAULT_BUF_SIZE, Some("utf-8".to_string()), inner)
    }

    /// Constructor allowing configuration of the size of the read buffer.
    ///
    /// Note that the actual capacity will always be at least as much as
    /// the number of bytes required to determine the encoding, but will
    /// never be less than the requested capacity.
    ///
    /// The input encoding will be detected using a standard heuristic,
    /// with no guiding user input.
    pub fn with_capacity(capacity: usize, inner: R) -> io::Result<Self> {
        Self::with_capacity_and_input_encoding(capacity, None, inner)
    }

    /// Constructor allowing caller to choose the input encoding, as well as
    /// set the read buffer capacity.
    pub fn with_capacity_and_input_encoding(
        capacity: usize,
        suggested_encoding: Option<String>,
        mut inner: R,
    ) -> io::Result<Self> {
        let (encoding, prebuf) = detect_encoding_with_suggestion(suggested_encoding, &mut inner)?;
        let decoder = encoding.get_decoder()?;

        // Initialize the input_buf from the pre-buffered data
        // if prebuf is bigger than the requested capacity, we'll increase the capacity to the size
        // of the pre-buffered data
        let mut input_buf: Vec<u8> = Vec::with_capacity(std::cmp::max(capacity, prebuf.len()));
        input_buf.extend(prebuf);

        Ok(XmlReadBuffer {
            inner,
            decoder,
            input_buf,
            output_buf: String::new(),
            output_pos: 0,
        })
    }

    fn fill_input_buf(&mut self) -> io::Result<usize> {
        if self.input_buf.is_empty() {
            let capacity = self.input_buf.capacity();
            // Read::read() ignores capacity, and reads from the beginning
            // of the used space to the end of the used space
            // So we need to force our input_buf's len() up to match capacity()
            if self.input_buf.len() < capacity {
                self.input_buf.resize(capacity, 0);
            }
            let read_size = self.inner.read(&mut self.input_buf)?;
            // The read may not have filled the buffer we gave it, so we need
            // to resize the buffer from capacity() down to read_size
            self.input_buf.resize(read_size, 0);
            Ok(read_size)
        } else {
            Ok(0)
        }
    }
}

impl<R: Read> Read for XmlReadBuffer<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let nread = {
            let mut rem = self.fill_buf()?;
            rem.read(buf)?
        };

        self.consume(nread);
        Ok(nread)
    }
}

impl<R: Read> BufRead for XmlReadBuffer<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.output_pos >= self.output_buf.len() {
            debug_assert!(self.output_pos == self.output_buf.len());
            self.fill_input_buf()?;
            // Take raw encoded data and convert it to utf-8
            let tmp_buf = self
                .decoder
                .decode(&self.input_buf, encoding::DecoderTrap::Strict)
                .map_err(|desc| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Input decoding error: {}", desc),
                    )
                })?;
            let mut seen_cr = false;
            // We do the more comprehensive xml 1.1 end-of-line handling rather than the sparser
            // xml 1.0 end-of-line, since it should be effective for both situations.
            // https://www.w3.org/TR/2006/REC-xml11-20060816/#sec-line-ends
            // The short version is that there's a list of characters we want to convert to LINE
            // FEED (0x0A), but there are some two-character sequences that need to be replaced
            // with a single LINE FEED, and they both start with CARRIAGE RETURN (0x0D), so we
            // always replace CR with LF, and if we see the second character of the two-character
            // sequence and we immediately saw a CR before them, it just gets omitted.
            // TODO: Eliminate an extra copy by moving the end-of-line normalization into the
            // "Read::read()" impl
            self.output_buf = tmp_buf
                .chars()
                .filter_map(|x| match x {
                    '\u{000d}' => {
                        seen_cr = true;
                        Some('\u{000a}')
                    }
                    '\u{000a}' | '\u{0085}' => {
                        if seen_cr {
                            seen_cr = false;
                            None
                        } else {
                            Some('\u{000a}')
                        }
                    }
                    '\u{2028}' => {
                        seen_cr = false;
                        Some('\u{000a}')
                    }
                    other => {
                        seen_cr = false;
                        Some(other)
                    }
                }).collect::<String>();
            self.input_buf.clear();
            self.output_pos = 0;
        }
        Ok(&self.output_buf.as_bytes()[self.output_pos..])
    }

    fn consume(&mut self, amt: usize) {
        self.output_pos = cmp::min(self.output_pos + amt, self.output_buf.len());
    }
}

impl<R> fmt::Debug for XmlReadBuffer<R>
where
    R: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("XmlReadBuffer")
            .field("reader", &self.inner)
            .field(
                "output_buf",
                &format_args!(
                    "{}/{}",
                    self.output_buf.len() - self.output_pos,
                    self.output_buf.len()
                ),
            ).finish()
    }
}

#[cfg(test)]
mod reader_tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_utf8() {
        // Test with no xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation = include_bytes!("../tests/validation/utf8.xml").to_vec();

        let utf8_bytes = include_bytes!("../tests/utf8/doc.xml").to_vec();
        match XmlReadBuffer::new(&utf8_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation = include_bytes!("../tests/validation/utf8_xmldecl.xml").to_vec();

        let utf8_bytes = include_bytes!("../tests/utf8/doc_xmldecl.xml").to_vec();
        match XmlReadBuffer::new(&utf8_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation =
            include_bytes!("../tests/validation/utf8_xmldecl_encodingdecl.xml").to_vec();

        let utf8_bytes = include_bytes!("../tests/utf8/doc_xmldecl_encodingdecl.xml").to_vec();
        match XmlReadBuffer::new(&utf8_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_with_bom() {
        // Test with no xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation = include_bytes!("../tests/validation/utf8.xml").to_vec();

        let utf8_with_bom_bytes = include_bytes!("../tests/utf8_bom/doc.xml").to_vec();
        match XmlReadBuffer::new(&utf8_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_with_bom_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation = include_bytes!("../tests/validation/utf8_xmldecl.xml").to_vec();

        let utf8_with_bom_bytes = include_bytes!("../tests/utf8_bom/doc_xmldecl.xml").to_vec();
        match XmlReadBuffer::new(&utf8_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_with_bom_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation =
            include_bytes!("../tests/validation/utf8_xmldecl_encodingdecl.xml").to_vec();

        let utf8_with_bom_bytes =
            include_bytes!("../tests/utf8_bom/doc_xmldecl_encodingdecl.xml").to_vec();
        match XmlReadBuffer::new(&utf8_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le() {
        // Test with no xmldecl, no encodingdecl
        let utf16_bytes = include_bytes!("../tests/utf16le/doc.xml").to_vec();
        // This test is *supposed* to produce an error when attempting to create the buffer because
        // there's not enough information to be confident about the encoding
        assert!(XmlReadBuffer::new(&utf16_bytes as &[u8]).is_err());
    }

    #[test]
    fn test_utf16le_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16le_xmldecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16le/doc_xmldecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation =
            include_bytes!("../tests/validation/utf16le_xmldecl_encodingdecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16le/doc_xmldecl_encodingdecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le_with_bom() {
        // Test with no xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16le.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16le_bom/doc.xml").to_vec();
        match XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le_with_bom_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16le_xmldecl.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16le_bom/doc_xmldecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le_with_bom_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation =
            include_bytes!("../tests/validation/utf16le_xmldecl_encodingdecl.xml").to_vec();

        let utf16_with_bom_bytes =
            include_bytes!("../tests/utf16le_bom/doc_xmldecl_encodingdecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be() {
        // Test with no xmldecl, no encodingdecl
        let utf16_bytes = include_bytes!("../tests/utf16be/doc.xml").to_vec();
        // This test is *supposed* to produce an error when attempting to create the buffer because
        // there's not enough information to be confident about the encoding
        assert!(XmlReadBuffer::new(&utf16_bytes as &[u8]).is_err());
    }

    #[test]
    fn test_utf16be_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16be_xmldecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16be/doc_xmldecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation =
            include_bytes!("../tests/validation/utf16be_xmldecl_encodingdecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16be/doc_xmldecl_encodingdecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be_with_bom() {
        // Test with no xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16be.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16be_bom/doc.xml").to_vec();
        match XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be_with_bom_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16be_xmldecl.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16be_bom/doc_xmldecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be_with_bom_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation =
            include_bytes!("../tests/validation/utf16be_xmldecl_encodingdecl.xml").to_vec();

        let utf16_with_bom_bytes =
            include_bytes!("../tests/utf16be_bom/doc_xmldecl_encodingdecl.xml").to_vec();
        match XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing XmlReadBuffer: {}", e),
        }
    }
}
