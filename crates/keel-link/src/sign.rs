//! Minimal linker ad-hoc signatures for Keel's thin arm64 executables.
//! Format reference: LLVM lld/MachO/SyntheticSections.cpp, CodeSignatureSection.
//! This is deliberately not a general Mach-O re-signer.
use crate::{PAGE, Result, fail};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom, Write};

const BLOCK: usize = 4096;
const IDENTIFIER: &[u8] = b"keel.native\0";
const BLOB_HEADER: usize = 20;
const DIRECTORY_HEADER: usize = 88; // CS_SUPPORTSEXECSEG (0x20400)
// Align the first hash to 16 bytes from the start of the SuperBlob, like lld.
const HASH_START: usize = (BLOB_HEADER + DIRECTORY_HEADER + IDENTIFIER.len() + 15) & !15;

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn le64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_be_bytes());
}
fn sizes(code_limit: u64) -> Result<(u32, u32)> {
    let slots = code_limit.div_ceil(BLOCK as u64);
    let size = (HASH_START as u64)
        .checked_add(slots.checked_mul(32).ok_or("signature size overflow")?)
        .ok_or("signature size overflow")?;
    // LC_CODE_SIGNATURE uses 32-bit offsets and lengths. Bound the complete
    // image too, rather than silently truncating either Mach-O field.
    if code_limit == 0
        || code_limit
            .checked_add(size)
            .is_none_or(|n| n > u32::MAX as u64)
    {
        return fail("signed Mach-O exceeds 32-bit file bounds");
    }
    Ok((slots as u32, size as u32))
}

pub(crate) fn sign<W: Read + Write + Seek>(image: &mut W) -> Result<()> {
    let code_limit = image.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let (slots, signature_size) = sizes(code_limit)?;
    if code_limit < PAGE as u64 || code_limit % 16 != 0 {
        return fail("signing requires a Keel unsigned Mach-O image");
    }
    image.rewind().map_err(|e| e.to_string())?;
    let mut header = [0; PAGE];
    image.read_exact(&mut header).map_err(|e| e.to_string())?;
    if le32(&header, 0) != 0xfeedfacf || le32(&header, 4) != 0x100000c || le32(&header, 12) != 2 {
        return fail("signing requires a thin arm64 Mach-O executable");
    }
    let count = le32(&header, 16);
    let commands_size = le32(&header, 20) as usize;
    if commands_size > PAGE - 32 - 16 {
        return fail("no room for LC_CODE_SIGNATURE in reserved header page");
    }
    let end = 32 + commands_size;
    let mut at = 32;
    let mut text = None;
    let mut linkedit = None;
    let mut uuid = false;
    for _ in 0..count {
        if at + 8 > end {
            return fail("invalid Mach-O load commands");
        }
        let kind = le32(&header, at);
        let size = le32(&header, at + 4) as usize;
        if size < 8 || size % 8 != 0 || size > end - at {
            return fail("invalid Mach-O load command size");
        }
        if kind == 0x1d {
            return fail("Mach-O image is already signed");
        }
        if kind == 0x1b && size == 24 {
            uuid = true;
        }
        if kind == 0x19 {
            if size < 72
                || (size - 72) / 80 != le32(&header, at + 64) as usize
                || (size - 72) % 80 != 0
            {
                return fail("invalid Mach-O segment command");
            }
            let file = le64(&header, at + 40);
            let len = le64(&header, at + 48);
            if file.checked_add(len).is_none_or(|n| n > code_limit) {
                return fail("Mach-O segment exceeds unsigned image");
            }
            // The new command must only consume reserved header padding.
            for section in (at + 72..at + size).step_by(80) {
                let offset = le32(&header, section + 48) as usize;
                if offset != 0 && offset < PAGE {
                    return fail("Mach-O section overlaps reserved header page");
                }
            }
            if &header[at + 8..at + 24] == b"__TEXT\0\0\0\0\0\0\0\0\0\0" {
                if text.replace((file, len)).is_some() || file != 0 || len < PAGE as u64 {
                    return fail("invalid Mach-O text segment");
                }
            }
            if &header[at + 8..at + 24] == b"__LINKEDIT\0\0\0\0\0\0" {
                if linkedit.replace((at, file, len)).is_some()
                    || file + len != code_limit
                    || size != 72
                {
                    return fail("invalid Mach-O linkedit segment");
                }
            }
        }
        at += size;
    }
    if at != end || !uuid || header[end..end + 16].iter().any(|b| *b != 0) {
        return fail("invalid Keel Mach-O header padding or UUID");
    }
    let (text_file, text_size) = text.ok_or("missing Mach-O text segment")?;
    let (segment, _, linkedit_size) = linkedit.ok_or("missing Mach-O linkedit segment")?;
    let new_size = linkedit_size + u64::from(signature_size);
    let vm_size = new_size.div_ceil(PAGE as u64) * PAGE as u64;
    header[segment + 32..segment + 40].copy_from_slice(&vm_size.to_le_bytes());
    header[segment + 48..segment + 56].copy_from_slice(&new_size.to_le_bytes());
    header[16..20].copy_from_slice(
        &count
            .checked_add(1)
            .ok_or("load command count overflow")?
            .to_le_bytes(),
    );
    header[20..24].copy_from_slice(&((commands_size + 16) as u32).to_le_bytes());
    for (offset, value) in [
        (0, 0x1d),
        (4, 16),
        (8, code_limit as u32),
        (12, signature_size),
    ] {
        header[end + offset..end + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    // The UUID was finalized by emit_to. All header changes must precede
    // page hashing; never patch a hashed byte after this point.
    image.rewind().map_err(|e| e.to_string())?;
    image.write_all(&header).map_err(|e| e.to_string())?;
    let mut signature = [0; HASH_START];
    put32(&mut signature, 0, 0xfade0cc0); // CSMAGIC_EMBEDDED_SIGNATURE
    put32(&mut signature, 4, signature_size);
    put32(&mut signature, 8, 1);
    put32(&mut signature, 12, 0); // CSSLOT_CODEDIRECTORY
    put32(&mut signature, 16, BLOB_HEADER as u32);
    let cd = &mut signature[BLOB_HEADER..];
    put32(cd, 0, 0xfade0c02);
    put32(cd, 4, signature_size - BLOB_HEADER as u32);
    put32(cd, 8, 0x20400);
    put32(cd, 12, 0x20002); // CS_ADHOC | CS_LINKER_SIGNED
    put32(cd, 16, (HASH_START - BLOB_HEADER) as u32);
    put32(cd, 20, DIRECTORY_HEADER as u32);
    put32(cd, 28, slots);
    put32(cd, 32, code_limit as u32);
    cd[36..40].copy_from_slice(&[32, 2, 0, 12]); // SHA256, 4 KiB pages
    put64(cd, 64, text_file);
    put64(cd, 72, text_size);
    put64(cd, 80, 1); // CS_EXECSEG_MAIN_BINARY
    cd[DIRECTORY_HEADER..DIRECTORY_HEADER + IDENTIFIER.len()].copy_from_slice(IDENTIFIER);
    image
        .seek(SeekFrom::Start(code_limit))
        .map_err(|e| e.to_string())?;
    image.write_all(&signature).map_err(|e| e.to_string())?;
    // Bounded reads and hash storage: batch 16 pages, including the final
    // short page without zero padding. No full-image or hash-table buffer.
    let mut pages = [0; BLOCK * 16];
    let mut hashes = [0; 32 * 16];
    let mut position = 0;
    while position < code_limit {
        let len = (code_limit - position).min(pages.len() as u64) as usize;
        image
            .seek(SeekFrom::Start(position))
            .map_err(|e| e.to_string())?;
        image
            .read_exact(&mut pages[..len])
            .map_err(|e| e.to_string())?;
        let mut hash_len = 0;
        for page in pages[..len].chunks(BLOCK) {
            hashes[hash_len..hash_len + 32].copy_from_slice(&Sha256::digest(page));
            hash_len += 32;
        }
        image
            .seek(SeekFrom::Start(
                code_limit + HASH_START as u64 + position / BLOCK as u64 * 32,
            ))
            .map_err(|e| e.to_string())?;
        image
            .write_all(&hashes[..hash_len])
            .map_err(|e| e.to_string())?;
        position += len as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signature_sizes_bound_the_complete_image_without_overflow() {
        assert_eq!(sizes(4096).unwrap(), (1, (HASH_START + 32) as u32));
        assert_eq!(sizes(4097).unwrap().0, 2);
        for limit in [0, u32::MAX as u64, u64::MAX] {
            assert!(sizes(limit).is_err());
        }
    }
    fn unsigned() -> Vec<u8> {
        let object = keel_asm::assemble_object(".text\n.globl _main\n_main:\nret\n").unwrap();
        crate::link(
            &[crate::Input {
                name: "main.o",
                bytes: &object,
            }],
            crate::Options {
                dylibs: vec![crate::Dylib {
                    install_name: "/usr/lib/libSystem.B.dylib".into(),
                    exports: Default::default(),
                    weak_exports: Default::default(),
                }],
                ..Default::default()
            },
        )
        .unwrap()
        .bytes
    }

    struct ShortIo {
        bytes: std::io::Cursor<Vec<u8>>,
        read_budget: usize,
        write_budget: usize,
    }
    impl Read for ShortIo {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            assert!(buffer.len() <= 65536, "unbounded signing read");
            if self.read_budget == 0 {
                return Err(std::io::Error::other("read failure"));
            }
            let len = buffer.len().min(113).min(self.read_budget);
            self.read_budget -= len;
            self.bytes.read(&mut buffer[..len])
        }
    }
    impl Write for ShortIo {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if self.write_budget == 0 {
                return Err(std::io::Error::other("write failure"));
            }
            let len = buffer.len().min(97).min(self.write_budget);
            self.write_budget -= len;
            self.bytes.write(&buffer[..len])
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Seek for ShortIo {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.bytes.seek(pos)
        }
    }
    #[test]
    fn short_io_and_exact_page_limit() {
        let mut bytes = unsigned();
        // Extend the final __LINKEDIT padding to an exact hash-page boundary.
        let end = bytes.len().div_ceil(BLOCK) * BLOCK;
        bytes.resize(end, 0);
        let mut at = 32;
        loop {
            if le32(&bytes, at) == 0x19 && &bytes[at + 8..at + 18] == b"__LINKEDIT" {
                let size = end as u64 - le64(&bytes, at + 40);
                bytes[at + 48..at + 56].copy_from_slice(&size.to_le_bytes());
                break;
            }
            at += le32(&bytes, at + 4) as usize;
        }
        let mut expected = std::io::Cursor::new(bytes.clone());
        sign(&mut expected).unwrap();
        let mut io = ShortIo {
            bytes: std::io::Cursor::new(bytes.clone()),
            read_budget: usize::MAX,
            write_budget: usize::MAX,
        };
        sign(&mut io).unwrap();
        assert_eq!(io.bytes.get_ref(), expected.get_ref());
        assert_eq!(io.bytes.position(), io.bytes.get_ref().len() as u64);
        let result = io.bytes.into_inner();
        let hashes = &result[end + HASH_START..];
        assert_eq!(hashes.len(), end / BLOCK * 32);
        for (page, hash) in result[..end].chunks(BLOCK).zip(hashes.chunks_exact(32)) {
            assert_eq!(Sha256::digest(page).as_slice(), hash);
        }
        for (read_budget, write_budget, error) in [
            (PAGE + 1, usize::MAX, "read failure"),
            (usize::MAX, PAGE + HASH_START + 1, "write failure"),
        ] {
            let mut io = ShortIo {
                bytes: std::io::Cursor::new(bytes.clone()),
                read_budget,
                write_budget,
            };
            assert!(sign(&mut io).unwrap_err().contains(error));
        }
    }

    #[test]
    fn rejects_malformed_headers_and_already_signed_images() {
        let bytes = unsigned();
        for offset in [0, 4, 12, 16, 20, 36] {
            let mut damaged = bytes.clone();
            damaged[offset..offset + 4].fill(255);
            assert!(sign(&mut std::io::Cursor::new(damaged)).is_err());
        }
        let mut image = std::io::Cursor::new(bytes);
        sign(&mut image).unwrap();
        assert!(sign(&mut image).is_err());
    }
}
