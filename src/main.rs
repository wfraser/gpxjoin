use anyhow::{bail, Context as _};
use quick_xml::{Reader, Writer};
use quick_xml::events::Event;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

trait StartsWithExt<U> {
    fn starts_with(&self, other: U) -> bool;
}

// Because the stdlib's slice::starts_with() doesn't work when the inner type is Vec<u8> and we're
// comparing against &[u8].
impl<S: AsRef<[u8]>> StartsWithExt<&[S]> for Vec<Vec<u8>> {
    fn starts_with(&self, other: &[S]) -> bool {
        if self.len() < other.len() {
            return false;
        }
        for (a, b) in self.iter().zip(other) {
            if a != b.as_ref() {
                return false;
            }
        }
        true
    }
}

fn parse_args() -> Vec<PathBuf> {
    let mut paths = vec![];
    let mut ignore_flags = false;
    for arg in std::env::args_os().skip(1) {
        if !ignore_flags {
            match arg.to_str() {
                Some("-h") | Some("--help") | Some("-V") | Some("--version") => {
                    eprintln!("gpxjoin v{} (c) 2021 {}",
                        env!("CARGO_PKG_VERSION"),
                        env!("CARGO_PKG_AUTHORS"));
                    eprintln!("usage: {} <file1.gpx> [<file2.gpx>, ...] > out.gpx",
                        std::env::args().next().unwrap());
                    eprintln!("Concatenates GPX files by appending tracks from subsequent GPX \
                        files after tracks from the first.\n\
                        Writes result to standard output.");
                    std::process::exit(1);
                }
                Some("--") => {
                    ignore_flags = true;
                    continue;
                }
                _ => ()
            }
        }
        paths.push(PathBuf::from(arg));
    }
    paths
}

pub fn join_gpx<R, W>(sources: impl Iterator<Item=R>, dest: W)
    -> anyhow::Result<()>
    where R: BufRead,
          W: Write,
{
    let mut first = None;
    let mut buf = vec![];
    let mut writer = Writer::new(dest);
    for source in sources {
        let mut path = vec![];
        let mut r = Reader::from_reader(source);
        loop {
            let evt = r.read_event(&mut buf)?;
            match evt {
                Event::Eof => break,
                Event::Start(ref start) => {
                    path.push(start.name().to_owned());
                }
                Event::End(_) if first.is_none() && path == [b"gpx"] => {
                    // Done with tracks, save the first file reader at this point and move on
                    // to the next file.
                    // Note we can only do this when we're sure there won't be additional track
                    // elements, hence why the check is when the path is just "gpx".
                    first = Some((r, evt.into_owned()));
                    break;
                }
                _ => (),
            }
            // If this is the first file, write everything, otherwise only write events if our path
            // begins with a track element.
            if first.is_none() || path.starts_with(&[b"gpx", b"trk"]) {
                writer.write_event(&evt)?;
            }
            if let Event::End(ref end) = evt {
                match path.pop() {
                    None => bail!("unexpected </{:?}> tag when path is empty", end.name()),
                    Some(popped) if popped != end.name() => {
                        bail!("start/end tag mismatch: expected </{:?}>, saw </{:?}>", popped, end.name());
                    }
                    _ => (),
                }
            }
            buf.clear();
        }
    }

    // Finish writing out the first file.
    let (mut first, stashed_evt) = first.unwrap();
    writer.write_event(stashed_evt)?;
    loop {
        let evt = first.read_event(&mut buf)?;
        if matches!(evt, Event::Eof) {
            break;
        }
        writer.write_event(evt)?;
        buf.clear();
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut files = vec![];
    for path in parse_args() {
        let r = BufReader::new(
                File::open(&path)
                    .with_context(|| format!("failed to open {:?}", path))?);
        files.push(r);
    }
    if files.is_empty() {
        bail!("need at least one source file");
    }
    join_gpx(files.into_iter(), io::stdout())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    #[test]
    fn test() {
        let mut a = Cursor::new(r#"<?xml version="1.0" encoding="utf-8"?>
<gpx version="1.1" creator="gpxjoin" xmlns="http://www.topografix.com/GPX/1/1">
    <metadata>
        <name><![CDATA[this is the first file]]></name>
        <desc>description here</desc>
        <author><name>whatever</name></author>
    </metadata>
    <trk>
        <name>first track</name>
        <trkseg>
            <trkpt lat="47.543448" lon="-121.096462">
                <ele>1008.620662</ele>
                <time>2021-08-27T18:59:24.070Z</time>
            </trkpt>
        </trkseg>
    </trk>
</gpx>
"#.as_bytes());
        let mut b = Cursor::new(r#"<?xml version="1.0" encoding="utf-8"?>
<gpx version="1.1" creator="gpxjoin" xmlns="http://www.topografix.com/GPX/1/1">
    <metadata>
        <name><![CDATA[this is the second file]]></name>
        <desc>description here</desc>
        <author><name>whatever</name></author>
    </metadata>
    <trk>
        <name>second track</name>
        <trkseg>
            <trkpt lat="47.552213" lon="-121.133853">
                <ele>1750.672203</ele>
                <time>2021-08-27T22:04:05.536Z</time>
            </trkpt>
        </trkseg>
    </trk>
</gpx>
"#.as_bytes());
        let mut out = Cursor::new(vec![]);
        join_gpx(std::array::IntoIter::new([&mut a, &mut b]), &mut out).unwrap();

        // Indentation at the second track is weird because XML is a bad format; there's no
        // reasonable way around it.
        assert_eq!(String::from_utf8(out.into_inner()).unwrap(),
            r#"<?xml version="1.0" encoding="utf-8"?>
<gpx version="1.1" creator="gpxjoin" xmlns="http://www.topografix.com/GPX/1/1">
    <metadata>
        <name><![CDATA[this is the first file]]></name>
        <desc>description here</desc>
        <author><name>whatever</name></author>
    </metadata>
    <trk>
        <name>first track</name>
        <trkseg>
            <trkpt lat="47.543448" lon="-121.096462">
                <ele>1008.620662</ele>
                <time>2021-08-27T18:59:24.070Z</time>
            </trkpt>
        </trkseg>
    </trk>
<trk>
        <name>second track</name>
        <trkseg>
            <trkpt lat="47.552213" lon="-121.133853">
                <ele>1750.672203</ele>
                <time>2021-08-27T22:04:05.536Z</time>
            </trkpt>
        </trkseg>
    </trk></gpx>
"#);
    }
}
