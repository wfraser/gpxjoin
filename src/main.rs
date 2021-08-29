use anyhow::{bail, Context as _};
use quick_xml::{Reader, Writer};
use quick_xml::events::Event;
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

fn main() -> anyhow::Result<()> {
    let mut writer = Writer::new(std::io::stdout());
    let mut first = None;
    let mut buf = vec![];
    for path in parse_args() {
        let mut r = Reader::from_file(&path)
            .with_context(|| format!("failed to open {:?}", path))?;

        let mut path = vec![];
        loop {
            let evt = r.read_event(&mut buf)?;
            if matches!(evt, Event::Eof) {
                break;
            }
            match evt {
                Event::Start(ref start) => {
                    path.push(start.name().to_owned());
                }
                Event::End(_) => {
                    if path == [b"gpx"] {
                        // Done with tracks, save the first file reader at this point and move on
                        // to the next file.
                        first = Some((r, evt.into_owned()));
                        break;
                    }
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
