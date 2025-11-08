//! EBML (Extensible Binary Meta Language) parser
//!
//! IETF RFC 8794 §3-5
//! Variable-length integer encoding for element IDs and sizes

use super::ElementId;
use av_io::{AsyncReadExt, Source};
use std::io::Result;

/// EBML element header
#[derive(Debug, Clone)]
pub struct EbmlElement {
    pub id: ElementId,
    pub size: u64,
    pub header_size: usize,
}

/// Read variable-length integer (VINT)
///
/// IETF RFC 8794 §4
/// First byte contains leading 1-bit marking the length,
/// followed by the value bits.
///
/// Examples:
/// - 0x81 = length 1, value 0x01
/// - 0x4001 = length 2, value 0x0001
/// - 0x200001 = length 3, value 0x000001
pub async fn read_vint(source: &mut dyn Source) -> Result<u64> {
    let first_byte = source.read_u8().await?;

    if first_byte == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid VINT: first byte is zero",
        ));
    }

    // Find leading 1-bit to determine length
    let leading_zeros = first_byte.leading_zeros();
    let length = (leading_zeros + 1) as usize;

    if length > 8 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "VINT length exceeds 8 bytes",
        ));
    }

    // Mask out the length marker bit
    let mask = 0xFF >> length;
    let mut value = (first_byte & mask) as u64;

    // Read remaining bytes
    for _ in 1..length {
        let byte = source.read_u8().await?;
        value = (value << 8) | (byte as u64);
    }

    Ok(value)
}

/// Read element ID (VINT encoding with marker bit preserved)
///
/// Element IDs keep the VINT length marker bit, unlike sizes
pub async fn read_element_id(source: &mut dyn Source) -> Result<ElementId> {
    let first_byte = source.read_u8().await?;

    if first_byte == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid Element ID: first byte is zero",
        ));
    }

    // Find length from leading zeros
    let leading_zeros = first_byte.leading_zeros();
    let length = (leading_zeros + 1) as usize;

    if length > 4 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Element ID length exceeds 4 bytes",
        ));
    }

    // For Element IDs, we keep the marker bit (don't mask it out)
    let mut id = first_byte as u32;

    // Read remaining bytes
    for _ in 1..length {
        let byte = source.read_u8().await?;
        id = (id << 8) | (byte as u32);
    }

    Ok(ElementId(id))
}

/// Read element size (VINT encoding)
pub async fn read_element_size(source: &mut dyn Source) -> Result<u64> {
    read_vint(source).await
}

/// Read EBML element header
pub async fn read_element_header(source: &mut dyn Source) -> Result<EbmlElement> {
    let id = read_element_id(source).await?;
    let size = read_element_size(source).await?;

    // Calculate header size (for seeking)
    let id_bytes = vint_length(id.0 as u64);
    let size_bytes = vint_length(size);
    let header_size = id_bytes + size_bytes;

    Ok(EbmlElement {
        id,
        size,
        header_size,
    })
}

/// Calculate VINT encoded length in bytes
fn vint_length(value: u64) -> usize {
    if value <= 0x7F {
        // 2^7 - 1
        1
    } else if value <= 0x3FFF {
        // 2^14 - 1
        2
    } else if value <= 0x1F_FFFF {
        // 2^21 - 1
        3
    } else if value <= 0x0FFF_FFFF {
        // 2^28 - 1
        4
    } else if value <= 0x07FF_FFFF_FF {
        // 2^35 - 1
        5
    } else if value <= 0x03FF_FFFF_FFFF {
        // 2^42 - 1
        6
    } else if value <= 0x01FF_FFFF_FFFF_FF {
        // 2^49 - 1
        7
    } else {
        // 2^56 - 1
        8
    }
}

/// Read unsigned integer from element data
pub async fn read_uint(source: &mut dyn Source, size: usize) -> Result<u64> {
    if size > 8 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Integer size exceeds 8 bytes",
        ));
    }

    let mut value = 0u64;
    for _ in 0..size {
        let byte = source.read_u8().await?;
        value = (value << 8) | (byte as u64);
    }

    Ok(value)
}

/// Read floating point from element data
pub async fn read_float(source: &mut dyn Source, size: usize) -> Result<f64> {
    match size {
        4 => {
            let bits = source.read_u32().await?;
            Ok(f32::from_bits(bits) as f64)
        }
        8 => {
            let bits = source.read_u64().await?;
            Ok(f64::from_bits(bits))
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid float size",
        )),
    }
}

/// Read string from element data
pub async fn read_string(source: &mut dyn Source, size: usize) -> Result<String> {
    let mut buf = vec![0u8; size];
    source.read_exact(&mut buf).await?;

    String::from_utf8(buf).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
    })
}

/// Skip element data
pub async fn skip_element(source: &mut dyn Source, element: &EbmlElement) -> Result<()> {
    use av_io::AsyncSeekExt;
    use std::io::SeekFrom;

    source
        .seek(SeekFrom::Current(element.size as i64))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vint_length() {
        assert_eq!(vint_length(0x01), 1);
        assert_eq!(vint_length(0x7F), 1);
        assert_eq!(vint_length(0x80), 2);
        assert_eq!(vint_length(0x3FFF), 2);
        assert_eq!(vint_length(0x4000), 3);
    }
}
