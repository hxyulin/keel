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

/// SHA-256 of each 4 KiB page of a stream, the last page unpadded, as the
/// code directory stores them.
#[derive(Default)]
pub(crate) struct PageHashes {
    hashes: Vec<[u8; 32]>,
    page: Sha256,
    filled: usize,
}
impl PageHashes {
    pub(crate) fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let take = data.len().min(BLOCK - self.filled);
            self.page.update(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == BLOCK {
                self.hashes.push(self.page.finalize_reset().into());
                self.filled = 0;
            }
        }
    }
    pub(crate) fn finish(mut self) -> Vec<[u8; 32]> {
        if self.filled != 0 {
            self.hashes.push(self.page.finalize().into());
        }
        self.hashes
    }
}

/// Hash large output images from immutable spans without assembling another
/// image buffer. Gaps are zero-filled, and pages retain their file order.
pub(crate) fn parallel_hash(spans: &[(usize, &[u8])], size: usize) -> Option<Vec<[u8; 32]>> {
    if crate::parallel::workers(size, 1024 * 1024) <= 1 {
        return None;
    }
    let mut previous = 0;
    for &(offset, data) in spans {
        let end = offset.checked_add(data.len())?;
        if offset < previous || end > size {
            return None;
        }
        previous = end;
    }
    let pages = size.div_ceil(BLOCK);
    let jobs = (0..pages)
        .step_by(256)
        .map(|start| start..(start + 256).min(pages))
        .collect();
    let chunks = crate::parallel::map(
        jobs,
        |r| r.len() * BLOCK,
        1024 * 1024,
        |range| {
            let mut span =
                spans.partition_point(|(offset, data)| offset + data.len() <= range.start * BLOCK);
            let mut hashes = Vec::with_capacity(range.len());
            for page in range {
                let start = page * BLOCK;
                let end = (start + BLOCK).min(size);
                while span < spans.len() && spans[span].0 + spans[span].1.len() <= start {
                    span += 1;
                }
                let hash = if let Some(&(offset, data)) = spans.get(span)
                    && offset <= start
                    && offset + data.len() >= end
                {
                    Sha256::digest(&data[start - offset..end - offset])
                } else {
                    let mut scratch = [0; BLOCK];
                    for &(offset, data) in &spans[span..] {
                        if offset >= end {
                            break;
                        }
                        let left = offset.max(start);
                        let right = (offset + data.len()).min(end);
                        scratch[left - start..right - start]
                            .copy_from_slice(&data[left - offset..right - offset]);
                    }
                    Sha256::digest(&scratch[..end - start])
                };
                hashes.push(hash.into());
            }
            hashes
        },
    );
    Some(chunks.into_iter().flatten().collect())
}

/// Signs an unsigned Keel image, reading it back to hash every page.
pub(crate) fn sign<W: Read + Write + Seek>(image: &mut W) -> Result<()> {
    sign_hashed(image, None)
}

/// Signs an unsigned Keel image. `pages`, if given, must be the page hashes
/// of the image as written. Signing rewrites the header page, so its hashes
/// are always recomputed; the others are trusted.
pub(crate) fn sign_hashed<W: Read + Write + Seek>(
    image: &mut W,
    pages: Option<&[[u8; 32]]>,
) -> Result<()> {
    let code_limit = image.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let (slots, signature_size) = sizes(code_limit)?;
    if pages.is_some_and(|pages| pages.len() != slots as usize) {
        return fail("page hashes do not match the unsigned image");
    }
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
        if size < 8 || !size.is_multiple_of(8) || size > end - at {
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
                || !(size - 72).is_multiple_of(80)
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
            if &header[at + 8..at + 24] == b"__TEXT\0\0\0\0\0\0\0\0\0\0"
                && (text.replace((file, len)).is_some() || file != 0 || len < PAGE as u64)
            {
                return fail("invalid Mach-O text segment");
            }
            if &header[at + 8..at + 24] == b"__LINKEDIT\0\0\0\0\0\0"
                && (linkedit.replace((at, file, len)).is_some()
                    || file + len != code_limit
                    || size != 72)
            {
                return fail("invalid Mach-O linkedit segment");
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
    // short page without zero padding. With precomputed hashes, only the
    // rewritten header page is read back.
    let mut buffer = [0; BLOCK * 16];
    let mut hashes = [0; 32 * 16];
    let mut position = 0;
    let reread = match pages {
        Some(_) => code_limit.min(PAGE as u64),
        None => code_limit,
    };
    while position < reread {
        let len = (reread - position).min(buffer.len() as u64) as usize;
        image
            .seek(SeekFrom::Start(position))
            .map_err(|e| e.to_string())?;
        image
            .read_exact(&mut buffer[..len])
            .map_err(|e| e.to_string())?;
        let mut hash_len = 0;
        for page in buffer[..len].chunks(BLOCK) {
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
    if let Some(pages) = pages {
        let rest = pages[(reread / BLOCK as u64) as usize..].as_flattened();
        image
            .seek(SeekFrom::Start(
                code_limit + HASH_START as u64 + reread / BLOCK as u64 * 32,
            ))
            .map_err(|e| e.to_string())?;
        image.write_all(rest).map_err(|e| e.to_string())?;
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
    #[test]
    fn precomputed_page_hashes_sign_like_reading_back() {
        let image = unsigned();
        assert!(image.len() > PAGE, "needs pages past the header page");
        let mut pages = PageHashes::default();
        // Uneven writes still hash whole 4 KiB pages.
        for chunk in image.chunks(1000) {
            pages.update(chunk);
        }
        let mut pages = pages.finish();
        let expected: Vec<[u8; 32]> = image
            .chunks(BLOCK)
            .map(|page| Sha256::digest(page).into())
            .collect();
        assert_eq!(pages, expected);
        let mut read_back = std::io::Cursor::new(image.clone());
        sign(&mut read_back).unwrap();
        // Signing rewrites the header page, so its stale hashes are ignored.
        pages[0] = [0xff; 32];
        let mut hashed = std::io::Cursor::new(image.clone());
        sign_hashed(&mut hashed, Some(&pages)).unwrap();
        assert_eq!(hashed.into_inner(), read_back.into_inner());
        pages.pop();
        let error = sign_hashed(&mut std::io::Cursor::new(image), Some(&pages)).unwrap_err();
        assert!(error.contains("page hashes"), "{error}");
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
        for (page, hash) in result[..end]
            .chunks(BLOCK)
            .zip(hashes.as_chunks::<32>().0.iter())
        {
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
    #[test]
    fn parallel_page_hashes_match_streaming_with_gaps_and_partial_pages() {
        let size: usize = 3 * 1024 * 1024 + 71;
        let data: Vec<_> = (0..size).map(|i| (i.wrapping_mul(17) >> 5) as u8).collect();
        let spans = [
            (0, &data[..91]),
            (4093, &data[4093..8199]),
            (12345, &data[12345..size - 31]),
        ];
        let mut image = vec![0; size];
        for &(at, bytes) in &spans {
            image[at..at + bytes.len()].copy_from_slice(bytes);
        }
        let mut serial = PageHashes::default();
        for chunk in image.chunks(777) {
            serial.update(chunk);
        }
        if let Some(parallel) = parallel_hash(&spans, size) {
            assert_eq!(parallel, serial.finish());
        }
        assert!(parallel_hash(&[(1, &data[..99]), (50, &data[..99])], size).is_none());
        assert!(parallel_hash(&[(size - 1, &data[..2])], size).is_none());
    }
}
