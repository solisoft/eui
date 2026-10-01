//! The compiled `AndroidManifest.xml` of a client package, and the four
//! values in it that make the package somebody's application: the package
//! name, the label, `versionName` and `versionCode`.
//!
//! A manifest in an APK is binary XML: a pool of strings, then elements
//! whose attributes point into the pool by index. So a new value is a new
//! string **appended** to the pool and the attribute pointed at it. Nothing
//! is replaced in place, because the pool is shared — aapt writes each
//! distinct string once, and a label that happened to equal some other
//! string in the file would otherwise change both. Appending moves no other
//! index, so every element after the pool is carried across byte for byte
//! but for the attributes being changed.
//!
//! The resource table is left alone, and that is how `aapt
//! --rename-manifest-package` has always done it: the package name in
//! `resources.arsc` is the one resources are compiled under, and Android
//! resolves `@mipmap/ic_launcher` by its id, not by that name.

const RES_XML: u16 = 0x0003;
const RES_STRING_POOL: u16 = 0x0001;
const RES_XML_START_ELEMENT: u16 = 0x0102;

/// A string, in `Res_value`.
const TYPE_STRING: u8 = 0x03;
/// A decimal integer, in `Res_value`.
const TYPE_INT_DEC: u8 = 0x10;

/// What to change. `None` leaves a value as the template had it.
#[derive(Debug, Clone, Default)]
pub struct Patch {
    /// `manifest@package`.
    pub package: Option<String>,
    /// `application@android:label`.
    pub label: Option<String>,
    /// `manifest@android:versionName`.
    pub version_name: Option<String>,
    /// `manifest@android:versionCode`.
    pub version_code: Option<u32>,
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, String> {
    match b.get(at..at + 2) {
        Some([x, y]) => Ok(u16::from_le_bytes([*x, *y])),
        _ => Err("the manifest is truncated".into()),
    }
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    match b.get(at..at + 4) {
        Some([a, b2, c, d]) => Ok(u32::from_le_bytes([*a, *b2, *c, *d])),
        _ => Err("the manifest is truncated".into()),
    }
}

fn put_u32(b: &mut [u8], at: usize, v: u32) -> Result<(), String> {
    b.get_mut(at..at + 4).ok_or("the manifest is truncated")?.copy_from_slice(&v.to_le_bytes());
    Ok(())
}

/// The strings of a pool chunk starting at `at`, and the chunk's length.
fn read_pool(b: &[u8], at: usize) -> Result<(Vec<String>, usize), String> {
    if u16_at(b, at)? != RES_STRING_POOL {
        return Err("the manifest does not start with a string pool".into());
    }
    let size = u32_at(b, at + 4)? as usize;
    let count = u32_at(b, at + 8)? as usize;
    let styles = u32_at(b, at + 12)?;
    let flags = u32_at(b, at + 16)?;
    let strings_start = u32_at(b, at + 20)? as usize;
    if styles != 0 {
        return Err("the manifest's string pool has styles; a manifest never does".into());
    }
    let utf8 = flags & 0x100 != 0;
    let header = u16_at(b, at + 2)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let mut p = at + strings_start + u32_at(b, at + header + 4 * i)? as usize;
        if utf8 {
            // Two lengths, each one or two bytes: characters, then bytes.
            let skip = |p: &mut usize| -> Result<usize, String> {
                let first = *b.get(*p).ok_or("the manifest is truncated")? as usize;
                *p += 1;
                if first & 0x80 != 0 {
                    let second = *b.get(*p).ok_or("the manifest is truncated")? as usize;
                    *p += 1;
                    Ok(((first & 0x7f) << 8) | second)
                } else {
                    Ok(first)
                }
            };
            skip(&mut p)?;
            let n = skip(&mut p)?;
            let s = b.get(p..p + n).ok_or("the manifest is truncated")?;
            out.push(String::from_utf8_lossy(s).into_owned());
        } else {
            let mut n = u16_at(b, p)? as usize;
            p += 2;
            if n & 0x8000 != 0 {
                n = ((n & 0x7fff) << 16) | u16_at(b, p)? as usize;
                p += 2;
            }
            let units: Vec<u16> = (0..n).map(|k| u16_at(b, p + 2 * k)).collect::<Result<_, _>>()?;
            out.push(String::from_utf16_lossy(&units));
        }
    }
    Ok((out, size))
}

/// A UTF-16 string pool holding `strings`, as a whole chunk.
fn write_pool(strings: &[String]) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::with_capacity(strings.len());
    for s in strings {
        offsets.push(u32::try_from(data.len()).unwrap_or(u32::MAX));
        let units: Vec<u16> = s.encode_utf16().collect();
        let n = units.len();
        if n > 0x7fff {
            data.extend_from_slice(&(u16::try_from((n >> 16) & 0x7fff).unwrap_or(0) | 0x8000).to_le_bytes());
            data.extend_from_slice(&u16::try_from(n & 0xffff).unwrap_or(0).to_le_bytes());
        } else {
            data.extend_from_slice(&u16::try_from(n).unwrap_or(0).to_le_bytes());
        }
        for u in units {
            data.extend_from_slice(&u.to_le_bytes());
        }
        data.extend_from_slice(&0u16.to_le_bytes());
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }
    let header = 28usize;
    let strings_start = header + 4 * strings.len();
    let size = strings_start + data.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&RES_STRING_POOL.to_le_bytes());
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&u32::try_from(size).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&u32::try_from(strings.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // styles
    out.extend_from_slice(&0u32.to_le_bytes()); // flags: UTF-16, not sorted
    out.extend_from_slice(&u32::try_from(strings_start).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // styles start
    for o in offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&data);
    out
}

/// What the manifest says, for a test or a message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    /// `manifest@package`.
    pub package: Option<String>,
    /// `application@android:label`, when it is a string and not a reference.
    pub label: Option<String>,
    /// `manifest@android:versionName`.
    pub version_name: Option<String>,
    /// `manifest@android:versionCode`.
    pub version_code: Option<u32>,
}

/// Every start element: `(offset of the chunk, element name)`.
fn elements(b: &[u8], strings: &[String], from: usize) -> Result<Vec<(usize, String)>, String> {
    let mut out = Vec::new();
    let mut at = from;
    while at + 8 <= b.len() {
        let kind = u16_at(b, at)?;
        let size = u32_at(b, at + 4)? as usize;
        if size < 8 {
            return Err("the manifest has an empty chunk".into());
        }
        if kind == RES_XML_START_ELEMENT {
            let name = u32_at(b, at + 20)? as usize;
            out.push((at, strings.get(name).cloned().unwrap_or_default()));
        }
        at += size;
    }
    Ok(out)
}

/// The attributes of the start element at `at`: `(offset, name, raw, type, data)`.
fn attributes(b: &[u8], strings: &[String], at: usize) -> Result<Vec<(usize, String, u8, u32)>, String> {
    let header = u16_at(b, at + 2)? as usize;
    let ext = at + header;
    let start = u16_at(b, ext + 8)? as usize;
    let size = u16_at(b, ext + 10)? as usize;
    let count = u16_at(b, ext + 12)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let a = ext + start + i * size;
        let name = u32_at(b, a + 4)? as usize;
        let kind = *b.get(a + 15).ok_or("the manifest is truncated")?;
        let data = u32_at(b, a + 16)?;
        out.push((a, strings.get(name).cloned().unwrap_or_default(), kind, data));
    }
    Ok(out)
}

/// What the manifest says now.
pub fn read(b: &[u8]) -> Result<Read, String> {
    if u16_at(b, 0)? != RES_XML {
        return Err("not a compiled Android manifest".into());
    }
    let header = u16_at(b, 2)? as usize;
    let (strings, pool) = read_pool(b, header)?;
    let mut r = Read::default();
    for (at, element) in elements(b, &strings, header + pool)? {
        for (_, name, kind, data) in attributes(b, &strings, at)? {
            let s = || strings.get(data as usize).cloned();
            match (element.as_str(), name.as_str(), kind) {
                ("manifest", "package", TYPE_STRING) => r.package = s(),
                ("manifest", "versionName", TYPE_STRING) => r.version_name = s(),
                ("manifest", "versionCode", TYPE_INT_DEC) => r.version_code = Some(data),
                ("application", "label", TYPE_STRING) => r.label = s(),
                _ => {}
            }
        }
    }
    Ok(r)
}

/// The manifest with `patch` applied.
pub fn patch(b: &[u8], patch: &Patch) -> Result<Vec<u8>, String> {
    if u16_at(b, 0)? != RES_XML {
        return Err("not a compiled Android manifest".into());
    }
    let header = u16_at(b, 2)? as usize;
    let (mut strings, pool) = read_pool(b, header)?;
    let rest_at = header + pool;
    let mut rest = b.get(rest_at..).ok_or("the manifest is truncated")?.to_vec();
    let found = elements(b, &strings, rest_at)?;

    // Each change: which element, which attribute, and the new value.
    let mut string_edits: Vec<(&str, &str, &str)> = Vec::new();
    if let Some(p) = &patch.package {
        string_edits.push(("manifest", "package", p));
    }
    if let Some(v) = &patch.version_name {
        string_edits.push(("manifest", "versionName", v));
    }
    if let Some(l) = &patch.label {
        string_edits.push(("application", "label", l));
    }
    for (element, attribute, value) in string_edits {
        let index = u32::try_from(strings.len()).map_err(|_| "the manifest has too many strings")?;
        let mut done = false;
        for (at, name) in &found {
            if name != element {
                continue;
            }
            for (a, attr, _, _) in attributes(b, &strings, *at)? {
                if attr == attribute {
                    let a = a - rest_at;
                    put_u32(&mut rest, a + 8, index)?; // rawValue
                    *rest.get_mut(a + 15).ok_or("the manifest is truncated")? = TYPE_STRING;
                    put_u32(&mut rest, a + 16, index)?; // data
                    done = true;
                }
            }
        }
        if !done {
            return Err(format!("the template's manifest has no {element}@{attribute} to set"));
        }
        strings.push(value.to_owned());
    }
    if let Some(code) = patch.version_code {
        let mut done = false;
        for (at, name) in &found {
            if name != "manifest" {
                continue;
            }
            for (a, attr, _, _) in attributes(b, &strings, *at)? {
                if attr == "versionCode" {
                    let a = a - rest_at;
                    put_u32(&mut rest, a + 8, u32::MAX)?; // no raw string
                    *rest.get_mut(a + 15).ok_or("the manifest is truncated")? = TYPE_INT_DEC;
                    put_u32(&mut rest, a + 16, code)?;
                    done = true;
                }
            }
        }
        if !done {
            return Err("the template's manifest has no versionCode to set".into());
        }
    }

    let pool = write_pool(&strings);
    let total = header + pool.len() + rest.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b.get(..header).ok_or("the manifest is truncated")?);
    put_u32(&mut out, 4, u32::try_from(total).map_err(|_| "the manifest is too large")?)?;
    out.extend_from_slice(&pool);
    out.extend_from_slice(&rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// The compiled manifest of the 0.7.1 release's `eui.apk`, as `cargo apk`
    /// and aapt wrote it.
    const TEMPLATE: &[u8] = include_bytes!("../tests/AndroidManifest-0.7.1.bin");

    #[test]
    fn the_template_reads() {
        let r = read(TEMPLATE).unwrap();
        assert_eq!(r.package.as_deref(), Some("org.eui.client"));
        assert_eq!(r.label.as_deref(), Some("EUI"));
        assert_eq!(r.version_name.as_deref(), Some("0.7.1"));
        assert_eq!(r.version_code, Some(0x0100_0701));
    }

    #[test]
    fn the_four_values_change_and_nothing_else_does() {
        let p = Patch { package: Some("com.example.mail".into()), label: Some("Courrier é".into()), version_name: Some("1.2.0".into()), version_code: Some(42) };
        let out = patch(TEMPLATE, &p).unwrap();
        let r = read(&out).unwrap();
        assert_eq!(r.package.as_deref(), Some("com.example.mail"));
        assert_eq!(r.label.as_deref(), Some("Courrier é"));
        assert_eq!(r.version_name.as_deref(), Some("1.2.0"));
        assert_eq!(r.version_code, Some(42));
        // Every element is still there, in order, with the same names.
        let names = |b: &[u8]| {
            let header = u16_at(b, 2).unwrap() as usize;
            let (s, pool) = read_pool(b, header).unwrap();
            elements(b, &s, header + pool).unwrap().into_iter().map(|(_, n)| n).collect::<Vec<_>>()
        };
        assert_eq!(names(TEMPLATE), names(&out));
        // The library the activity loads is untouched: same string, same value.
        let header = u16_at(&out, 2).unwrap() as usize;
        let (s, _) = read_pool(&out, header).unwrap();
        assert!(s.iter().any(|x| x == "android.app.NativeActivity"));
        assert!(s.iter().any(|x| x == "eui"));
        assert_eq!(u32_at(&out, 4).unwrap() as usize, out.len());
    }

    #[test]
    fn nothing_asked_is_nothing_changed_but_the_pool_encoding() {
        let out = patch(TEMPLATE, &Patch::default()).unwrap();
        assert_eq!(read(&out).unwrap(), read(TEMPLATE).unwrap());
    }
}
