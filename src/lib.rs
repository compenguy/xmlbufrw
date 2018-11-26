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
            self.output_buf = self
                .decoder
                .decode(&self.input_buf, encoding::DecoderTrap::Strict)
                .map_err(|desc| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Input decoding error: {}", desc),
                    )
                })?;
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
        let utf8_str = std::str::from_utf8(include_bytes!("../tests/utf8/doc.xml"))
            .expect("Unexpected error reading in utf-8 data");
        let utf8_bytes = include_bytes!("../tests/utf8/doc.xml").to_vec();
        let mut decoding_reader =
            XmlReadBuffer::new(&utf8_bytes as &[u8]).expect("Failed initializing XmlReadBuffer");
        let mut utf8_encoded_doc: String = String::new();
        decoding_reader
            .read_to_string(&mut utf8_encoded_doc)
            .expect("Failed decoding input data");
        assert_eq!(utf8_str, utf8_encoded_doc);
    }

    #[test]
    fn test_utf8_with_bom() {
        let utf8_str = std::str::from_utf8(include_bytes!("../tests/utf8/doc.xml"))
            .expect("Unexpected error reading in utf-8 data");
        let utf8_with_bom_bytes = include_bytes!("../tests/utf8_bom/doc.xml").to_vec();
        let mut decoding_reader = XmlReadBuffer::new(&utf8_with_bom_bytes as &[u8])
            .expect("Failed initializing XmlReadBuffer");
        let mut utf8_encoded_doc: String = String::new();
        decoding_reader
            .read_to_string(&mut utf8_encoded_doc)
            .expect("Failed decoding input data");
        assert_eq!(utf8_str, utf8_encoded_doc);
    }

    #[test]
    fn test_utf16le() {
        let utf8_str = std::str::from_utf8(include_bytes!("../tests/utf8/doc.xml"))
            .expect("Unexpected error reading in utf-8 data");
        let utf16_bytes = include_bytes!("../tests/utf16le/doc.xml").to_vec();

        let mut decoding_reader =
            XmlReadBuffer::new(&utf16_bytes as &[u8]).expect("Failed initializing XmlReadBuffer");
        let mut utf8_encoded_doc: String = String::new();
        decoding_reader
            .read_to_string(&mut utf8_encoded_doc)
            .expect("Failed reading recoded data");
        assert_eq!(utf8_str, utf8_encoded_doc);
    }

    #[test]
    fn test_utf16le_with_bom() {
        let utf8_str = std::str::from_utf8(include_bytes!("../tests/utf8/doc.xml"))
            .expect("Unexpected error reading in utf-8 data");
        let utf16_with_bom_bytes = include_bytes!("../tests/utf16le_bom/doc.xml").to_vec();

        let mut decoding_reader = XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8])
            .expect("Failed initializing XmlReadBuffer");
        let mut utf8_encoded_doc: String = String::new();
        decoding_reader
            .read_to_string(&mut utf8_encoded_doc)
            .expect("Failed reading recoded data");
        assert_eq!(utf8_str, utf8_encoded_doc);
    }

    #[test]
    fn test_utf16be() {
        let utf8_str = std::str::from_utf8(include_bytes!("../tests/utf8/doc.xml"))
            .expect("Unexpected error reading in utf-8 data");
        let utf16_bytes = include_bytes!("../tests/utf16be/doc.xml").to_vec();

        let mut decoding_reader =
            XmlReadBuffer::new(&utf16_bytes as &[u8]).expect("Failed initializing XmlReadBuffer");
        let mut utf8_encoded_doc: String = String::new();
        decoding_reader
            .read_to_string(&mut utf8_encoded_doc)
            .expect("Failed reading recoded data");
        assert_eq!(utf8_str, utf8_encoded_doc);
    }

    #[test]
    fn test_utf16be_with_bom() {
        let utf8_str = std::str::from_utf8(include_bytes!("../tests/utf8/doc.xml"))
            .expect("Unexpected error reading in utf-8 data");
        let utf16_with_bom_bytes = include_bytes!("../tests/utf16be_bom/doc.xml").to_vec();

        let mut decoding_reader = XmlReadBuffer::new(&utf16_with_bom_bytes as &[u8])
            .expect("Failed initializing XmlReadBuffer");
        let mut utf8_encoded_doc: String = String::new();
        decoding_reader
            .read_to_string(&mut utf8_encoded_doc)
            .expect("Failed reading recoded data");
        assert_eq!(utf8_str, utf8_encoded_doc);
    }
}
