//! Raw printing to a Brother QL label printer.
//!
//! The UI draws the label and sends it as 1-bit rows; this wraps them in the QL raster commands and hands the bytes
//! to the Windows spooler untouched ("RAW"), so the normal Brother driver can stay installed and no print dialog,
//! page size or scaling setting gets in the way.

/// Head width of the QL-700 in dots (90 bytes per raster line).
const HEAD_DOTS: usize = 720;
/// Blank tape the printer feeds before and after each label, in dots.
const FEED_MARGIN: u16 = 35;

/// Tape width in mm, printable dots, and the right-hand offset in dots (continuous DK rolls).
const TAPES: &[(u8, usize, usize)] = &[(29, 306, 6), (38, 413, 12), (50, 554, 12), (54, 590, 0), (62, 696, 12)];

pub fn tape_dots(mm: u8) -> Option<(usize, usize)> {
    TAPES.iter().find(|t| t.0 == mm).map(|t| (t.1, t.2))
}

/// The QL command stream for `pages` labels of continuous tape. Each page is `lines` rows of `ceil(width / 8)` bytes,
/// most significant bit first, 1 for black.
pub fn raster(tape_mm: u8, width: usize, pages: &[&[u8]]) -> Result<Vec<u8>, String> {
    let (dots, offset) = tape_dots(tape_mm).ok_or_else(|| format!("unsupported tape width {tape_mm} mm"))?;
    if width != dots {
        return Err(format!("a {tape_mm} mm tape prints {dots} dots across, the label is {width}"));
    }
    let stride = width.div_ceil(8);
    let mut out = vec![0u8; 200]; // invalidate whatever half-sent job the printer holds
    out.extend_from_slice(&[0x1B, 0x40]); // initialize
    for (n, page) in pages.iter().enumerate() {
        if page.is_empty() || !page.len().is_multiple_of(stride) {
            return Err("label data isn't a whole number of rows".into());
        }
        let lines = (page.len() / stride) as u32;
        // media and quality: type, width, length valid; high quality; continuous tape; width; length 0; line count
        out.extend_from_slice(&[0x1B, 0x69, 0x7A, 0xCE, 0x0A, tape_mm, 0x00]);
        out.extend_from_slice(&lines.to_le_bytes());
        out.extend_from_slice(&[u8::from(n > 0), 0x00]);
        out.extend_from_slice(&[0x1B, 0x69, 0x4D, 0x40]); // auto cut on
        out.extend_from_slice(&[0x1B, 0x69, 0x41, 0x01]); // cut after every label
        out.extend_from_slice(&[0x1B, 0x69, 0x4B, 0x08]); // cut at end
        out.extend_from_slice(&[0x1B, 0x69, 0x64]);
        out.extend_from_slice(&FEED_MARGIN.to_le_bytes());
        out.extend_from_slice(&[0x4D, 0x00]); // no compression
        for row in page.chunks(stride) {
            out.extend_from_slice(&[0x67, 0x00, (HEAD_DOTS / 8) as u8]);
            out.extend_from_slice(&head_row(row, width, offset));
        }
        out.push(if n + 1 == pages.len() { 0x1A } else { 0x0C });
    }
    Ok(out)
}

/// One raster line for the print head: the label's row placed against the right edge (less the tape's margin), then
/// the whole line mirrored, the way the printer reads it.
fn head_row(row: &[u8], width: usize, offset: usize) -> [u8; HEAD_DOTS / 8] {
    let left = HEAD_DOTS - width - offset;
    let mut out = [0u8; HEAD_DOTS / 8];
    for x in 0..width {
        if row[x / 8] & (0x80 >> (x % 8)) != 0 {
            let mirrored = HEAD_DOTS - 1 - (left + x);
            out[mirrored / 8] |= 0x80 >> (mirrored % 8);
        }
    }
    out
}

/// Printers installed on this PC, for the picker.
#[cfg(windows)]
pub fn printers() -> Vec<String> {
    win::printers()
}

#[cfg(not(windows))]
pub fn printers() -> Vec<String> {
    Vec::new()
}

#[cfg(windows)]
pub fn send(printer: &str, data: &[u8]) -> Result<(), String> {
    win::send(printer, data)
}

#[cfg(not(windows))]
pub fn send(_printer: &str, _data: &[u8]) -> Result<(), String> {
    Err("Printing straight to the label printer is only set up for Windows. Use Save label image instead.".into())
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    #[repr(C)]
    struct DocInfo1 {
        name: *const u16,
        output: *const u16,
        datatype: *const u16,
    }

    #[repr(C)]
    struct PrinterInfo4 {
        name: *mut u16,
        server: *mut u16,
        attributes: u32,
    }

    #[link(name = "winspool")]
    extern "system" {
        fn OpenPrinterW(name: *const u16, handle: *mut *mut c_void, defaults: *const c_void) -> i32;
        fn ClosePrinter(handle: *mut c_void) -> i32;
        fn StartDocPrinterW(handle: *mut c_void, level: u32, info: *const DocInfo1) -> u32;
        fn EndDocPrinter(handle: *mut c_void) -> i32;
        fn StartPagePrinter(handle: *mut c_void) -> i32;
        fn EndPagePrinter(handle: *mut c_void) -> i32;
        fn WritePrinter(handle: *mut c_void, buf: *const c_void, len: u32, written: *mut u32) -> i32;
        fn EnumPrintersW(flags: u32, name: *const u16, level: u32, buf: *mut u8, size: u32, needed: *mut u32, returned: *mut u32) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn printers() -> Vec<String> {
        const LOCAL_AND_CONNECTIONS: u32 = 0x02 | 0x04;
        let (mut needed, mut returned) = (0u32, 0u32);
        // SAFETY: the first call only asks how much room the list needs.
        unsafe { EnumPrintersW(LOCAL_AND_CONNECTIONS, std::ptr::null(), 4, null_mut(), 0, &mut needed, &mut returned) };
        if needed == 0 {
            return Vec::new();
        }
        // Aligned for the structs the system writes into it.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        // SAFETY: `buf` holds `needed` bytes; the system fills it with `returned` PRINTER_INFO_4 records plus their strings.
        let ok = unsafe { EnumPrintersW(LOCAL_AND_CONNECTIONS, std::ptr::null(), 4, buf.as_mut_ptr().cast(), needed, &mut needed, &mut returned) };
        if ok == 0 {
            return Vec::new();
        }
        let infos = buf.as_ptr().cast::<PrinterInfo4>();
        let mut names = Vec::new();
        for i in 0..returned as usize {
            // SAFETY: `i < returned`, and each name is a NUL-terminated string inside `buf`.
            unsafe {
                let p = (*infos.add(i)).name;
                if p.is_null() {
                    continue;
                }
                let mut len = 0;
                while *p.add(len) != 0 {
                    len += 1;
                }
                names.push(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
            }
        }
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    pub fn send(printer: &str, data: &[u8]) -> Result<(), String> {
        let name = wide(printer);
        let doc = wide("QSL label");
        let kind = wide("RAW");
        let mut handle: *mut c_void = null_mut();
        // SAFETY: plain spooler calls with NUL-terminated strings that outlive them; the handle is closed on every path.
        unsafe {
            if OpenPrinterW(name.as_ptr(), &mut handle, std::ptr::null()) == 0 {
                return Err(format!("Can't open the printer \"{printer}\". Is it installed and switched on?"));
            }
            let info = DocInfo1 { name: doc.as_ptr(), output: std::ptr::null(), datatype: kind.as_ptr() };
            let result = if StartDocPrinterW(handle, 1, &info) == 0 {
                Err(format!("The printer \"{printer}\" wouldn't start the job."))
            } else {
                let mut result = Ok(());
                if StartPagePrinter(handle) == 0 {
                    result = Err(format!("The printer \"{printer}\" wouldn't start the page."));
                } else {
                    let mut written = 0u32;
                    if WritePrinter(handle, data.as_ptr().cast(), data.len() as u32, &mut written) == 0 || written as usize != data.len() {
                        result = Err(format!("Sending the label to \"{printer}\" failed. Is it switched on and loaded with tape?"));
                    }
                    EndPagePrinter(handle);
                }
                EndDocPrinter(handle);
                result
            };
            ClosePrinter(handle);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_has_one_command_block_and_whole_rows_per_label() {
        // 554 dots across = 70 bytes a row, 3 rows.
        let page = vec![0u8; 70 * 3];
        let out = raster(50, 554, &[&page, &page]).unwrap();
        assert_eq!(&out[..200], &[0u8; 200][..]);
        assert_eq!(&out[200..202], &[0x1B, 0x40]);
        // First label: 3 lines, first page; second: 3 lines, not first.
        let at = |from: usize| out[from..].windows(4).position(|w| w == [0x1B, 0x69, 0x7A, 0xCE]).unwrap() + from;
        let one = at(202);
        assert_eq!(&out[one + 4..one + 7], &[0x0A, 50, 0]);
        assert_eq!(&out[one + 7..one + 11], &3u32.to_le_bytes());
        assert_eq!(out[one + 11], 0);
        let two = at(one + 4);
        assert_eq!(out[two + 11], 1);
                assert_eq!(*out.last().unwrap(), 0x1A);
        // Every raster line is 3 + 90 bytes.
        assert!(out.windows(3).filter(|w| *w == [0x67, 0x00, 90]).count() >= 6);
    }

    #[test]
    fn row_sits_against_the_right_edge_and_is_mirrored() {
        // Only the label's first dot is black.
        let mut row = vec![0u8; 70];
        row[0] = 0x80;
        let out = head_row(&row, 554, 12);
        // left = 720 - 554 - 12 = 154; mirrored = 719 - 154 = 565.
        let mut want = [0u8; 90];
        want[565 / 8] = 0x80 >> (565 % 8);
        assert_eq!(out, want);
    }

    #[test]
    fn wrong_width_is_refused() {
        assert!(raster(50, 500, &[&[0u8; 63]]).is_err());
        assert!(raster(51, 554, &[&[0u8; 70]]).is_err());
        assert!(raster(50, 554, &[&[0u8; 71]]).is_err());
    }
}
