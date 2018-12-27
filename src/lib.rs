/// This crate provides xml-specific file reading capabilities, in addition to converting from
/// a range of input encodings to rust-friendly utf-8.
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
extern crate encoding_rs;
extern crate encodingbufrw;

mod enc_detect;
pub mod reader;
// TODO: pub mod writer
