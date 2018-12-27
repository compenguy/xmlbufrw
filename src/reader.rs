use enc_detect::detect_encoding_with_suggestion;

use encodingbufrw::reader::CodecReadBuffer;
use encodingbufrw::DEFAULT_BUF_SIZE;

use std::io;

pub fn new<R: std::io::Read>(inner: R) -> io::Result<CodecReadBuffer<R>> {
    with_capacity_and_input_encoding(inner, DEFAULT_BUF_SIZE, None)
}

pub fn with_capacity<R: std::io::Read>(
    inner: R,
    capacity: usize,
) -> io::Result<CodecReadBuffer<R>> {
    with_capacity_and_input_encoding(inner, capacity, None)
}

pub fn with_capacity_and_input_encoding<R: std::io::Read>(
    mut inner: R,
    capacity: usize,
    suggested_encoding: Option<String>,
) -> io::Result<CodecReadBuffer<R>> {
    let (encoding, prebuf) = detect_encoding_with_suggestion(suggested_encoding, &mut inner)?;
    let encoding_name = encoding.get_name();

    // Initialize the input_buf from the pre-buffered data
    // if prebuf is bigger than the requested capacity, we'll increase the capacity to the size
    // of the pre-buffered data
    let mut input_buf: Vec<u8> = Vec::with_capacity(std::cmp::max(capacity, prebuf.len()));
    input_buf.extend(prebuf);

    CodecReadBuffer::for_encoding_with_initial_buffer(inner, &encoding_name, input_buf)
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
        match new(&utf8_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation = include_bytes!("../tests/validation/utf8_xmldecl.xml").to_vec();

        let utf8_bytes = include_bytes!("../tests/utf8/doc_xmldecl.xml").to_vec();
        match new(&utf8_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation =
            include_bytes!("../tests/validation/utf8_xmldecl_encodingdecl.xml").to_vec();

        let utf8_bytes = include_bytes!("../tests/utf8/doc_xmldecl_encodingdecl.xml").to_vec();
        match new(&utf8_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_with_bom() {
        // Test with no xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation = include_bytes!("../tests/validation/utf8.xml").to_vec();

        let utf8_with_bom_bytes = include_bytes!("../tests/utf8_bom/doc.xml").to_vec();
        match new(&utf8_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf8_with_bom_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf8_validation = include_bytes!("../tests/validation/utf8_xmldecl.xml").to_vec();

        let utf8_with_bom_bytes = include_bytes!("../tests/utf8_bom/doc_xmldecl.xml").to_vec();
        match new(&utf8_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
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
        match new(&utf8_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf8_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le() {
        // Test with no xmldecl, no encodingdecl
        let utf16_bytes = include_bytes!("../tests/utf16le/doc.xml").to_vec();
        // This test is *supposed* to produce an error when attempting to create the buffer because
        // there's not enough information to be confident about the encoding
        assert!(new(&utf16_bytes as &[u8]).is_err());
    }

    #[test]
    fn test_utf16le_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16le_xmldecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16le/doc_xmldecl.xml").to_vec();
        match new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation =
            include_bytes!("../tests/validation/utf16le_xmldecl_encodingdecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16le/doc_xmldecl_encodingdecl.xml").to_vec();
        match new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le_with_bom() {
        // Test with no xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16le.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16le_bom/doc.xml").to_vec();
        match new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16le_with_bom_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16le_xmldecl.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16le_bom/doc_xmldecl.xml").to_vec();
        match new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
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
        match new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be() {
        // Test with no xmldecl, no encodingdecl
        let utf16_bytes = include_bytes!("../tests/utf16be/doc.xml").to_vec();
        // This test is *supposed* to produce an error when attempting to create the buffer because
        // there's not enough information to be confident about the encoding
        assert!(new(&utf16_bytes as &[u8]).is_err());
    }

    #[test]
    fn test_utf16be_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16be_xmldecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16be/doc_xmldecl.xml").to_vec();
        match new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be_xmldecl_encodingdecl() {
        // Test with xmldecl, encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation =
            include_bytes!("../tests/validation/utf16be_xmldecl_encodingdecl.xml").to_vec();

        let utf16_bytes = include_bytes!("../tests/utf16be/doc_xmldecl_encodingdecl.xml").to_vec();
        match new(&utf16_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be_with_bom() {
        // Test with no xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16be.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16be_bom/doc.xml").to_vec();
        match new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }

    #[test]
    fn test_utf16be_with_bom_xmldecl() {
        // Test with xmldecl, no encodingdecl
        // Validation docs have the same text as the test docs, but always in utf-8, no bom
        let utf16_validation = include_bytes!("../tests/validation/utf16be_xmldecl.xml").to_vec();

        let utf16_with_bom_bytes = include_bytes!("../tests/utf16be_bom/doc_xmldecl.xml").to_vec();
        match new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
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
        match new(&utf16_with_bom_bytes as &[u8]) {
            Ok(mut decoding_reader) => {
                let mut utf8_encoded_doc: String = String::new();
                decoding_reader
                    .read_to_string(&mut utf8_encoded_doc)
                    .expect("Failed decoding input data");
                assert_eq!(&utf16_validation, &utf8_encoded_doc.as_bytes());
            }
            Err(e) => panic!("Failed initializing read buffer: {}", e),
        }
    }
}
