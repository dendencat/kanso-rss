use crate::{NewSubscription, Subscription};
use anyhow::{bail, Result};
use quick_xml::{events::Event, Reader};

pub const MAX_OPML_BYTES: usize = 1024 * 1024;

pub fn parse(xml: &str) -> Result<Vec<NewSubscription>> {
    if xml.len() > MAX_OPML_BYTES {
        bail!("OPML exceeds 1 MiB");
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    let mut folders: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut items = Vec::new();
    loop {
        let event = reader.read_event()?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::DocType(_) => bail!("DTD is prohibited"),
            Event::Start(ref e) | Event::Empty(ref e) => {
                // The root must be OPML; the reader validates closing names.
                if depth == 0 && root_seen {
                    bail!("Multiple XML roots");
                }
                if !root_seen {
                    if e.name().as_ref() != b"opml" {
                        bail!("Expected OPML document");
                    }
                    root_seen = true;
                }
                if e.name().as_ref() == b"outline" {
                    let mut title = String::new();
                    let mut folder = String::new();
                    let mut url = None;
                    for attribute in e.attributes() {
                        let a = attribute?;
                        let value = a
                            .decoded_and_normalized_value(
                                quick_xml::XmlVersion::Implicit1_0,
                                reader.decoder(),
                            )?
                            .into_owned();
                        match a.key.as_ref() {
                            b"title" => title = value,
                            b"text" if title.is_empty() => title = value,
                            b"xmlUrl" => url = Some(value),
                            _ => {}
                        }
                    }
                    if let Some(url) = url {
                        crate::fetch::validate_url(&url)?;
                        items.push(NewSubscription {
                            url,
                            title,
                            folder: folders
                                .iter()
                                .filter(|f| !f.is_empty())
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(" / "),
                        });
                        if items.len() > 500 {
                            bail!("OPML exceeds 500 subscriptions");
                        }
                    } else {
                        folder = title;
                    }
                    if !empty {
                        folders.push(folder);
                    }
                }
                if !empty {
                    depth += 1;
                }
                if depth > 20 {
                    bail!("OPML nesting is too deep");
                }
            }
            Event::End(e) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| anyhow::anyhow!("Invalid nesting"))?;
                if e.name().as_ref() == b"outline" {
                    folders.pop();
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_seen || depth != 0 {
        bail!("Incomplete OPML document");
    }
    Ok(items)
}

pub fn export(feeds: &[Subscription]) -> String {
    let esc = |s: &str| quick_xml::escape::escape(s).into_owned();
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\"><head><title>Kanso subscriptions</title></head><body>\n");
    let folders: std::collections::BTreeSet<_> = feeds.iter().map(|f| f.folder.as_str()).collect();
    for folder in folders {
        if !folder.is_empty() {
            xml.push_str(&format!("<outline text=\"{}\">\n", esc(folder)));
        }
        for feed in feeds.iter().filter(|f| f.folder == folder) {
            xml.push_str(&format!(
                "<outline type=\"rss\" text=\"{}\" title=\"{}\" xmlUrl=\"{}\"/>\n",
                esc(&feed.title),
                esc(&feed.title),
                esc(&feed.url)
            ));
        }
        if !folder.is_empty() {
            xml.push_str("</outline>\n");
        }
    }
    xml.push_str("</body></opml>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supports_folders_and_rejects_entities() {
        let items = parse(r#"<opml version="2.0"><body><outline text="Tech &amp; Science"><outline text="Rust" xmlUrl="https://example.org/feed"/></outline></body></opml>"#).unwrap();
        assert_eq!(items[0].folder, "Tech & Science");
        assert!(parse(
            r#"<!DOCTYPE opml [<!ENTITY x SYSTEM "file:///etc/passwd">]><opml><body/></opml>"#
        )
        .is_err());
        assert!(parse("<opml><body>").is_err());
        let deep = format!(
            "<opml>{}{}</opml>",
            "<outline>".repeat(25),
            "</outline>".repeat(25)
        );
        assert!(parse(&deep).is_err());
    }
    #[test]
    fn exports_roundtrip_without_markup_injection() {
        let feed = Subscription {
            id: "a".into(),
            url: "https://example.org/rss?a=1&b=2".into(),
            title: "\"/><script>".into(),
            folder: "Tech".into(),
            unread: 0,
            last_fetched: None,
            last_error: None,
        };
        let parsed = parse(&export(&[feed])).unwrap();
        assert_eq!(parsed[0].title, "\"/><script>");
        assert_eq!(parsed[0].folder, "Tech");
    }
}
