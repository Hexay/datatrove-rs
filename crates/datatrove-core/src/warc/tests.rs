use super::{Item, WarcRecords};

fn record(rec_type: &str, mime: Option<&str>, block: &str) -> String {
    let mime = mime.map(|m| format!("WARC-Identified-Payload-Type: {m}\r\n")).unwrap_or_default();
    format!(
        "WARC/1.0\r\nWARC-Type: {rec_type}\r\nWARC-Record-ID: <urn:uuid:1>\r\nWARC-Target-URI: https://a.com/\r\n\
         WARC-Date: 2024-01-01T00:00:00Z\r\n{mime}Content-Length: {}\r\n\r\n{block}\r\n\r\n",
        block.len()
    )
}

/// (kind, index, body) per item: kind is 'd'oc, 'r'aw or 'f'allback.
fn items(data: &str) -> Vec<(char, usize, String)> {
    let mut recs = WarcRecords::new(data.as_bytes());
    let mut out = Vec::new();
    loop {
        match recs.next_item() {
            Item::End => return out,
            Item::Doc(d) => out.push(('d', d.index, String::from_utf8_lossy(d.body).into_owned())),
            Item::Raw { index, .. } => out.push(('r', index, String::new())),
            Item::Fallback { index } => out.push(('f', index, String::new())),
        }
    }
}

#[test]
fn selects_html_responses_and_strips_http_headers() {
    let data = [
        record("request", None, "GET / HTTP/1.1\r\n\r\n"),
        record("response", Some("text/html"), "HTTP/1.1 200 OK\r\nX: 1\r\n  cont\r\n\r\n<p>hi</p>"),
        record("response", Some("image/png"), "HTTP/1.1 200 OK\r\n\r\nPNG"),
        record("response", None, "HTTP/1.1 200 OK\r\n\r\n<p>?</p>"),
        record("conversion", Some("text/plain"), "plain"),
    ]
    .concat();
    let expected = [('d', 1, "<p>hi</p>".into()), ('r', 3, String::new()), ('d', 4, "plain".into())];
    assert_eq!(items(&data), expected);
}

#[test]
fn encoded_payloads_are_raw() {
    let data = record("response", Some("text/html"), "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\nxx");
    assert_eq!(items(&data), [('r', 0, String::new())]);
}

#[test]
fn malformed_framing_falls_back_at_the_record() {
    let good = record("response", Some("text/html"), "HTTP/1.1 200 OK\r\n\r\nok");
    let bad_trailer = good.replace("ok\r\n\r\n", "okjunk\r\n\r\n");
    assert_eq!(items(&format!("{good}{bad_trailer}{good}")), [('d', 0, "ok".into()), ('f', 1, String::new())]);
    let truncated = &good[..good.len() - 8];
    assert_eq!(items(&format!("{good}{truncated}")), [('d', 0, "ok".into()), ('f', 1, String::new())]);
    assert_eq!(items(&format!("\r\n{good}")), [('f', 0, String::new())]);
}
