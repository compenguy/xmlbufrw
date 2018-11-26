use std::io;
use std::io::Read;

use encoding;
use encoding::types::EncodingRef;

// Implements the encoding detection heuristic suggested by
// https://www.w3.org/TR/xml/#sec-guessing
pub fn detect_encoding_with_suggestion<R: Read>(
    suggested_encoding: Option<String>,
    reader: &mut R,
) -> io::Result<(Encoding, Vec<u8>)> {
    let mut prebuf: Vec<u8> = Vec::with_capacity(64);
    // Check the first four bytes
    let mut quad = [0; 4];
    reader.take(quad.len() as u64).read_exact(&mut quad)?;

    let (encoding_guess, bom_bytes) = Encoding::new_from_buffer(&quad[0..4])?;
    // Add all bytes after the bom (if present) to the prebuf
    prebuf.extend(&quad[bom_bytes..]);

    // Buffer for reading a-char-at-a-time until we have enough to see if there's an xmldecl
    // checking for "<?xml" followed by a whitespace char, so we need to fetch six chars, minus
    // however many chars are already in prebuf
    let char_width = encoding_guess.get_char_width();
    let mut tmp_buf: Vec<u8> = vec![0; (6 * char_width) - prebuf.len()];
    reader.take(tmp_buf.len() as u64).read_exact(&mut tmp_buf)?;
    prebuf.extend(&tmp_buf);

    let temp_decoder = encoding_guess.get_decoder()?;

    let mut xml_decl = temp_decoder
        .decode(&prebuf, encoding::DecoderTrap::Strict)
        .map_err(|desc| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Input decoding error: {}", desc),
            )
        })?;

    let has_xml_decl = xml_decl.starts_with("<?xml") && xml_decl
        .chars()
        .nth(5)
        .map(char::is_whitespace)
        .unwrap_or(false);
    if !has_xml_decl {
        // If there's no xmldecl, suggestion takes priority
        // https://www.w3.org/TR/xml/#sec-guessing-with-ext-info
        if let Some(encoding_name) = suggested_encoding {
            let encoding = Encoding::new_from_name(&encoding_name, true)?;
            return Ok((encoding, prebuf));
        }
        // if no xmldecl, not definitive, and no suggested encoding then error
        if !encoding_guess.is_definitive() {
            return Err(io::Error::new(io::ErrorKind::Other, "Unable to detect input file encoding.  No Byte Order Mark, and no xml declaration."));
        }
        // if no xmldecl, the encoding detection was definitive, and no suggested encoding, return now
        if encoding_guess.is_definitive() {
            return Ok((encoding_guess, prebuf));
        }
    }

    // Now we have to read through until we get to the end of the xmldecl - "?>"
    let mut one_char_buf: Vec<u8> = vec![0; encoding_guess.get_char_width()];
    while !xml_decl.ends_with("?>") {
        reader
            .take(one_char_buf.len() as u64)
            .read_exact(&mut one_char_buf)?;
        prebuf.extend(&one_char_buf);
        let next_char = temp_decoder
            .decode(&one_char_buf, encoding::DecoderTrap::Strict)
            .map_err(|desc| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Input decoding error: {}", desc),
                )
            })?;
        xml_decl.push_str(&next_char);
        // we don't have a full state machine here to detect if we're running through valid
        // xml_decl data, so we're just going to put a hard upper cap at 256 chars - if we've
        // made it this far without finding "?>", we're giving up
        if xml_decl.len() > 256 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Input format error: input doesn't appear to be valid xml.",
            ));
        }
    }

    let xml_decl_tokens = xml_decl
        .split_whitespace()
        .flat_map(|attr| attr.split('='))
        .filter(|t| !t.is_empty());
    let mut encoding_tokens = xml_decl_tokens.skip_while(|t| t != &"encoding");
    if encoding_tokens.next().is_none() {
        // No encoding name in xmldecl
        return Ok((encoding_guess, prebuf));
    }

    if let Some(encoding_val) = encoding_tokens.next() {
        // if definitive and xmldecl, error if encodingdecl doesn't match detected encoding
        // get value between the quotes
        let mut encoding_val_iter = encoding_val.chars();
        let starting_quote = encoding_val_iter.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "Improperly formatted encodingdecl: unquoted value.",
            )
        })?;
        let encoding_name = encoding_val_iter
            .take_while(|c| c != &starting_quote)
            .collect::<String>();

        if encoding_guess.is_definitive() {
            if !encoding_guess.encoding_decl_is_compatible(&encoding_name)? {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Detected input encoding {} is incompatible with declared encoding {}",
                        encoding_guess.get_name(),
                        encoding_name
                    ),
                ));
            }
            return Ok((encoding_guess, prebuf));
        } else {
            // if not definitive, and xmldecl, return xmldecl encoding
            return Ok((encoding_guess, prebuf));
        }
    }

    Ok((Encoding::new_from_name("utf-8", false)?, prebuf))
}

pub enum Encoding {
    Ascii(bool),
    Utf8(bool),
    Utf16Le(bool),
    Utf16Be(bool),
    // These are encodings that we can guess, but for which we don't have a 
    // decoder, so we won't emit these
    /*
    Utf32Le(bool),
    Utf32Be(bool),
    UtfEbcdic(bool),
    EbcdicCpUs(bool),
    */
}

impl Encoding {
    pub fn new_from_buffer(buf: &[u8]) -> io::Result<(Self, usize)> {
        match buf[0..4] {
            // Byte Order Mark test
            // UTF-8
            [0xEF, 0xBB, 0xBF, _] => Ok((Self::new_from_name("utf-8", true)?, 3)),
            // UTF-16, little-endian
            [0xFF, 0xFE, _po3, _po4] if _po3 != 0x00 && _po4 == 0x00 => {
                Ok((Self::new_from_name("utf-16le", true)?, 2))
            }
            // UTF-16, big-endian
            [0xFE, 0xFF, _po3, _po4] if _po3 == 0x00 && _po4 != 0x00 => {
                Ok((Self::new_from_name("utf-16be", true)?, 2))
            }
            /*
            // UCS-4, little endian (4321 order)
            [0xFF, 0xFE, 0x00, 0x00] => Ok((Self::new_from_name("utf-32le", true)?, 4)),
            // UCS-4, big endian (1234 order)
            [0x00, 0x00, 0xFE, 0xFF] => Ok((Self::new_from_name("utf-32be", true)?, 4)),
            // UCS-4, unusual octet order (2143 order)
            [0x00, 0x00, 0xFF, 0xFE] => Err(io::Error::new(io::ErrorKind::Other, "Unsupported file encoding, \"UCS-4 unusual octet order (2143 order)\"")),
            // UCS-4, little endian (3412 order)
            [0xFE, 0xFF, 0x00, 0x00] => Err(io::Error::new(io::ErrorKind::Other, "Unsupported file encoding, \"UCS-4 unusual octet order (3412 order)\"")),
            // UTF-EBCDIC
            [0xDD, 0x73, 0x66, 0x73] => Ok((Self::new_from_name("utf-ebcdic", true)?, 4)),
            */

            // xmldecl char-width/endianness test
            // UTF-8, ISO 646, ASCII, ISO 8859, etc '<?xm'
            // encodingDecl required
            [0x3C, 0x3F, 0x78, 0x6D] => Ok((Self::new_from_name("utf-8", false)?, 0)),
            // UTF-16, little-endian '<?'
            [0x3C, 0x00, 0x3F, 0x00] => Ok((Self::new_from_name("utf-16le", true)?, 0)),
            // UTF-16, big-endian '<?'
            [0x00, 0x3C, 0x00, 0x3F] => Ok((Self::new_from_name("utf-16be", true)?, 0)),
            /*
            // UCS-4, little endian (4321 order) '<'
            [0x3C, 0x00, 0x00, 0x00] => Ok((Self::new_from_name("utf-32le", true)?, 0)),
            // UCS-4, big endian (1234 order) '<'
            [0x00, 0x00, 0x00, 0x3C] => Ok((Self::new_from_name("utf-32be", true)?, 0)),
            // UCS-4, unusual octet order (2143 order) '<'
            [0x00, 0x00, 0x3C, 0x00] => Err(io::Error::new(io::ErrorKind::Other, "Unsupported file encoding, \"UCS-4 unusual octet order (2143 order)\"")),
            // UCS-4, little endian (3412 order) '<'
            [0x00, 0x3C, 0x00, 0x00] => Err(io::Error::new(io::ErrorKind::Other, "Unsupported file encoding, \"UCS-4 unusual octet order (3412 order)\"")),
            // Some flavor of EBCDIC '<?xm'
            // encodingDecl required
            [0x4C, 0x6F, 0xA7, 0x94] => Ok((Self::new_from_name("ebcdic-cp-us", false)?, 0)),
            */

            // Any remaining multibyte encodings are unsupported
            [0x00, _, _, _] | [_, 0x00, _, _] | [_, _, 0x00, _] | [_, _, _, 0x00] => {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Unsupported multi-byte file encoding.",
                ))
            }
            // No BOM, document doesn't immediately start with xml declaration, but it appears to
            // be a single-byte encoding, so we'll assume utf-8 and hope for the best
            _ => Ok((Self::new_from_name("utf-8", false)?, 0)),
        }
    }

    pub fn new_from_name(name: &str, is_definitive: bool) -> io::Result<Self> {
        if let Some(decoder) = encoding::label::encoding_from_whatwg_label(name) {
            match decoder.name().to_lowercase().as_str() {
                "ascii" => Ok(Encoding::Ascii(is_definitive)),
                "utf-8" => Ok(Encoding::Utf8(is_definitive)),
                "utf-16le" => Ok(Encoding::Utf16Le(is_definitive)),
                "utf-16be" => Ok(Encoding::Utf16Be(is_definitive)),
                enc => Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unsupported encoding requested: {}", enc),
                )),
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Unsupported encoding requested: {}", name),
            ))
        }
    }

    pub fn get_decoder(&self) -> io::Result<EncodingRef> {
        encoding::label::encoding_from_whatwg_label(&self.get_name()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Unrecognized input encoding name: {}", self.get_name()),
            )
        })
    }

    pub fn get_name(&self) -> String {
        match self {
            Encoding::Ascii(_) => "ascii".to_string(),
            Encoding::Utf8(_) => "utf-8".to_string(),
            Encoding::Utf16Le(_) => "utf-16le".to_string(),
            Encoding::Utf16Be(_) => "utf-16be".to_string(),
            /*
            Encoding::Utf32Le(_) => "utf-32le".to_string(),
            Encoding::Utf32Be(_) => "utf-32be".to_string(),
            Encoding::UtfEbcdic(_) => "utf-ebcdic".to_string(),
            Encoding::EbcdicCpUs(_) => "ebcdic-cp-us".to_string(),
            */
        }
    }

    pub fn get_char_width(&self) -> usize {
        match self {
            Encoding::Ascii(_) => 1,
            Encoding::Utf8(_) => 1,
            Encoding::Utf16Le(_) => 2,
            Encoding::Utf16Be(_) => 2,
            /*
            Encoding::Utf32Le(_) => 4,
            Encoding::Utf32Be(_) => 4,
            Encoding::UtfEbcdic(_) => 1,
            Encoding::EbcdicCpUs(_) => 1,
            */
        }
    }

    pub fn is_definitive(&self) -> bool {
        match self {
            Encoding::Ascii(is_definitive)
            | Encoding::Utf8(is_definitive)
            | Encoding::Utf16Le(is_definitive)
            | Encoding::Utf16Be(is_definitive) => *is_definitive,
            /*
            Encoding::Utf32Le(is_definitive) |
            Encoding::Utf32Be(is_definitive) |
            Encoding::UtfEbcdic(is_definitive) |
            Encoding::EbcdicCpUs(is_definitive) => *is_definitive,
            */
        }
    }

    pub fn encoding_decl_is_compatible(&self, encoding_decl_name: &str) -> io::Result<bool> {
        let other_decoder = encoding::label::encoding_from_whatwg_label(encoding_decl_name)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unrecognized input encoding name: {}", encoding_decl_name),
                )
            })?;

        let self_name = self.get_name();
        let other_name = other_decoder.name();

        // This takes care of all UTF-16 cases
        if self_name == other_name {
            return Ok(true);
        }

        // BOM was present, but the requested name doesn't match up with
        // the BOM
        if self.is_definitive() && self_name != other_name {
            return Ok(false);
        }

        // non-definitive encodings are ascii, utf-8, and ebcdic-cp-us
        // and non-definitive detection of ascii and utf-8 are basically
        // interchangeable
        // Basically, the two are compatible if we're ascii or utf-8, and they're any of the
        // supported singlebyte encodings here:
        // https://lifthrasiir.github.io/rust-encoding/src/encoding/src/all.rs.html#41-69
        if self_name == "utf-8" || self_name == "ascii" {
            let compat = match other_name {
                "ascii" => true,
                "utf-8" => true,
                "ibm866" => true,
                "iso-8859-1" => true,
                "iso-8859-2" => true,
                "iso-8859-3" => true,
                "iso-8859-4" => true,
                "iso-8859-5" => true,
                "iso-8859-6" => true,
                "iso-8859-7" => true,
                "iso-8859-8" => true,
                "iso-8859-10" => true,
                "iso-8859-13" => true,
                "iso-8859-14" => true,
                "iso-8859-15" => true,
                "iso-8859-16" => true,
                "koi8-r" => true,
                "koi8-u" => true,
                "mac-roman" => true,
                "windows-874" => true,
                "windows-1250" => true,
                "windows-1251" => true,
                "windows-1252" => true,
                "windows-1253" => true,
                "windows-1254" => true,
                "windows-1255" => true,
                "windows-1256" => true,
                "windows-1257" => true,
                "windows-1258" => true,
                "mac-cyrillic" => true,
                _ => false,
            };
            Ok(compat)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, format!("Unable to determine compatibility of detected encoding {} and declared encoding {}", self_name, other_name)))
        }
    }
}
