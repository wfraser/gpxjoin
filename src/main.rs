use anyhow::{bail, Context as _};
use quick_xml::{Reader, Writer};
use quick_xml::events::Event;
use std::path::Path;

trait StartsWithExt<U> {
    fn starts_with(&self, other: U) -> bool;
}

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

fn main() -> anyhow::Result<()> {
    let mut writer = Writer::new(std::io::stdout());
    let mut first = None;
    let mut buf = vec![];
    for path in std::env::args_os().skip(1) {
        let mut r = Reader::from_file(&Path::new(&path))
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
                    if path == &[b"gpx"] {
                        // Done with tracks, save the first file reader at this point.
                        first = Some((r, evt.into_owned()));
                        break;
                    }
                }
                _ => (),
            }
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
