use std::io;
use std::io::Read;

use encoding_rs;

pub fn decoder_helper(decoder: &mut encoding_rs::Decoder, input: &[u8]) -> io::Result<String> {
    let mut decoded = String::with_capacity(input.len() * 4);

    let (result, bytes_read) =
        decoder.decode_to_string_without_replacement(&input, &mut decoded, false);
    if let encoding_rs::DecoderResult::Malformed(_, _) = result {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Malformed input. {:x?}, position {}.", input, bytes_read),
        ))
    } else {
        Ok(decoded)
    }
}

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

    // Extend the prebuf with enough chars to have gotten the xmldecl prefix
    let xml_decl_prefix = "<?xml ";
    let char_width = encoding_guess.get_char_width();
    let xml_decl_prefix_width = xml_decl_prefix.len() * char_width;
    // Buffer for reading a-char-at-a-time until we have enough to see if there's an xmldecl
    let mut tmp_buf: Vec<u8> = vec![0; xml_decl_prefix_width - prebuf.len()];
    reader.take(tmp_buf.len() as u64).read_exact(&mut tmp_buf)?;
    prebuf.extend(&tmp_buf);

    let mut temp_decoder = encoding_guess.get_decoder()?;

    // Because we don't yet *know* that we're inside an xmldecl, and outside of an xmldecl a
    // display char may consist of more than one utf char, we're going to decode this one step
    // at a time.
    // make an iterator over chunks of char_width size, decode it
    let has_xml_decl: bool = prebuf
        .chunks(char_width)
        .map(|x| decoder_helper(&mut temp_decoder, x))
        .zip(xml_decl_prefix.chars())
        .all(|(input_char_str_result, decl_char)| {
            if let Ok(input_char_str) = input_char_str_result {
                (char::is_whitespace(decl_char) && input_char_str.chars().all(char::is_whitespace))
                    || (decl_char.to_string() == input_char_str)
            } else {
                false
            }
        });

    // How to resolve suggested encoding with document inferences:
    // https://www.w3.org/TR/xml/#sec-guessing-with-ext-info
    if !has_xml_decl {
        // If there's no xmldecl, but there is a BOM, rely on that
        if encoding_guess.is_definitive() {
            return Ok((encoding_guess, prebuf));
        } else if let Some(encoding_name) = suggested_encoding {
            // If there's no xmldecl, and no BOM, fall back on the suggested encoding
            let encoding = Encoding::new_from_name(&encoding_name, true)?;
            return Ok((encoding, prebuf));
        } else {
            // if no xmldecl, no BOM, and no suggested encoding then error
            return Err(io::Error::new(io::ErrorKind::Other, "Unable to detect input file encoding.  No Byte Order Mark, and no xml declaration."));
        }
    }
    let mut xml_decl = decoder_helper(&mut temp_decoder, &prebuf)?;

    // Now we have to read through until we get to the end of the xmldecl - "?>"
    let mut one_char_buf: Vec<u8> = vec![0; encoding_guess.get_char_width()];
    while !xml_decl.ends_with("?>") {
        reader
            .take(one_char_buf.len() as u64)
            .read_exact(&mut one_char_buf)?;
        prebuf.extend(&one_char_buf);
        let next_char = decoder_helper(&mut temp_decoder, &one_char_buf)?;
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
        let mut encoding_val_iter = encoding_val.chars();
        let starting_quote = encoding_val_iter.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                "Improperly formatted encodingdecl: unquoted value.",
            )
        })?;
        let encoding_name: String = encoding_val_iter
            .take_while(|c| c != &starting_quote)
            .collect::<String>();

        // if definitive and xmldecl, error if encodingdecl doesn't match detected encoding
        // get value between the quotes
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

    if let Some(encoding_name) = suggested_encoding {
        Ok((Encoding::new_from_name(&encoding_name, false)?, prebuf))
    } else {
        Ok((Encoding::new_from_name("utf-8", false)?, prebuf))
    }
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
                    "Missing BOM and no xml declaration, or Unsupported multi-byte file encoding.",
                ))
            }
            // No BOM, document doesn't immediately start with xml declaration, but it appears to
            // be a single-byte encoding, so we'll assume utf-8 and hope for the best
            _ => Ok((Self::new_from_name("utf-8", false)?, 0)),
        }
    }

    pub fn new_from_name(name: &str, is_definitive: bool) -> io::Result<Self> {
        if let Some(encoding) = encoding_rs::Encoding::for_label_no_replacement(name.as_bytes()) {
            match encoding.name().to_lowercase().as_str() {
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

    pub fn get_decoder(&self) -> io::Result<encoding_rs::Decoder> {
        encoding_rs::Encoding::for_label_no_replacement(&self.get_name().as_bytes())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unrecognized input encoding name: {}", self.get_name()),
                )
            }).map(|enc| enc.new_decoder_without_bom_handling())
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
        let other_decoder =
            encoding_rs::Encoding::for_label_no_replacement(encoding_decl_name.as_bytes())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Unrecognized input encoding name: {}", encoding_decl_name),
                    )
                })?;

        let self_name = self.get_name().to_lowercase();
        let other_name = other_decoder.name().to_lowercase();

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
        // https://docs.rs/encoding_rs/0.8.13/src/encoding_rs/lib.rs.html
        // look for LABELS_SORTED
        if self_name == "utf-8" || self_name == "ascii" {
            let compat = match other_name.as_str() {
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
