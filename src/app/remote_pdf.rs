//! Validate and open a remote PDF without creating a local file.

use std::{
    io::{self, Read},
    sync::Arc,
    time::Duration,
};
use url::Url;

pub(super) fn parse_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value.trim()).ok().filter(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
    });
    url.ok_or_else(|| "Enter an http:// or https:// URL without embedded credentials.".into())
}

#[derive(Debug)]
pub(super) enum DownloadError {
    NotPdf,
    Failed(String),
}

impl From<io::Error> for DownloadError {
    fn from(error: io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

const LIMIT: u64 = 256 * 1024 * 1024;
const TOO_LARGE: &str = "This PDF exceeds the 256 MiB download limit.";

fn read_signature(stream: &mut impl Read) -> Result<[u8; 5], DownloadError> {
    let mut header = [0; 5];
    match stream.read_exact(&mut header) {
        Ok(()) if &header == b"%PDF-" => Ok(header),
        Ok(()) => Err(DownloadError::NotPdf),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Err(DownloadError::NotPdf),
        Err(e) => Err(e.into()),
    }
}

pub(super) fn download(mut url: Url) -> Result<Arc<crate::pdf::Document>, DownloadError> {
    url.set_fragment(None);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .max_redirects(5)
        .build()
        .into();
    let mut response = agent
        .get(url.as_str())
        .call()
        .map_err(|error| match error {
            ureq::Error::StatusCode(code) => {
                DownloadError::Failed(format!("Server returned HTTP {code}."))
            }
            _ => DownloadError::Failed(
                "Could not download the PDF. Check the address and connection.".into(),
            ),
        })?;
    if !response.status().is_success() {
        return Err(DownloadError::Failed(format!(
            "Server returned HTTP {}.",
            response.status()
        )));
    }
    let length = response
        .headers()
        .get("Content-Length")
        .and_then(|value| value.to_str().ok()?.parse::<u64>().ok());
    let mut stream = response.body_mut().as_reader();
    let header = read_signature(&mut stream)?;
    if length.is_some_and(|len| len > LIMIT) {
        return Err(DownloadError::Failed(TOO_LARGE.into()));
    }
    let name = url
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .and_then(|name| {
            name.get(..name.len().checked_sub(4)?)
                .filter(|_| name.to_ascii_lowercase().ends_with(".pdf"))
        })
        .unwrap_or("document");
    // Prefix the basename also to avoid reserved Windows device names.
    let name: String = name
        .chars()
        .take(80)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut bytes = header.to_vec();
    let remaining = LIMIT - header.len() as u64;
    if stream.take(remaining + 1).read_to_end(&mut bytes)? as u64 > remaining {
        return Err(DownloadError::Failed(TOO_LARGE.into()));
    }
    let document = crate::pdf::Document::from_bytes(format!("Downloaded-{name}.pdf").into(), bytes)
        .map_err(DownloadError::Failed)?;
    if document.pages().is_empty() {
        return Err(DownloadError::Failed("PDF has no pages".into()));
    }
    Ok(Arc::new(document))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        io::{BufRead, Write},
        net::TcpListener,
    };

    #[test]
    fn rejects_a_non_pdf_without_consuming_the_body() {
        let mut body = io::Cursor::new(b"<html>the rest of a long response");
        assert!(matches!(
            read_signature(&mut body),
            Err(DownloadError::NotPdf)
        ));
        assert_eq!(body.position(), 5);
    }

    /// An extensionless endpoint deliberately serving misleading metadata.
    pub(crate) fn serve(body: Vec<u8>, requests: usize) -> Url {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = Url::parse(&format!(
            "http://{}/download?id=1",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        std::thread::spawn(move || {
            for _ in 0..requests {
                let (mut socket, _) = listener.accept().unwrap();
                let mut reader = io::BufReader::new(&socket);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() != 0 && line != "\r\n" {
                    line.clear();
                }
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                socket.write_all(&body).unwrap();
            }
        });
        url
    }

    #[test]
    fn imports_pdf_and_discards_corrupt_or_non_pdf_bodies() {
        let pdf = std::fs::read("examples/readme-sample.pdf").unwrap();
        let copy = download(serve(pdf.clone(), 1)).unwrap();
        assert_eq!(copy.bytes(), pdf);
        assert!(matches!(
            download(serve(b"<html>".to_vec(), 1)),
            Err(DownloadError::NotPdf)
        ));
        assert!(matches!(
            download(serve(b"%PDF-broken".to_vec(), 1)),
            Err(DownloadError::Failed(_))
        ));
    }
}
