//! Bounded local MP4 reads for the active task; arbitrary paths are never accepted.
use crate::session::AppSession;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use tauri::http::{Request, Response, StatusCode};

const MAX_CHUNK: u64 = 2 * 1024 * 1024;

fn byte_range(value: &str, length: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    if length == 0 || value.contains(',') {
        return None;
    }
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?.min(length);
        if suffix == 0 {
            return None;
        }
        (length - suffix, length - 1)
    } else {
        let start = start.parse::<u64>().ok()?;
        let end = if end.is_empty() {
            length - 1
        } else {
            end.parse::<u64>().ok()?.min(length - 1)
        };
        (start, end)
    };
    (start <= end && start < length).then(|| (start, end.min(start.saturating_add(MAX_CHUNK - 1))))
}

fn error(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", "*")
        .body(Vec::new())
        .unwrap()
}

pub fn serve(session: &AppSession, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if request.method() != "GET" && request.method() != "HEAD" {
        return error(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Some(id) = request
        .uri()
        .path()
        .strip_prefix('/')
        .and_then(|path| path.strip_suffix(".mp4"))
    else {
        return error(StatusCode::NOT_FOUND);
    };
    let Ok(path) = session.recording_media_path(id) else {
        return error(StatusCode::NOT_FOUND);
    };
    let Ok(mut file) = File::open(path) else {
        return error(StatusCode::NOT_FOUND);
    };
    let Ok(metadata) = file.metadata() else {
        return error(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let length = metadata.len();
    if length == 0 {
        return error(StatusCode::NOT_FOUND);
    }
    let mut response = Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Accept-Ranges", "bytes")
        .header("Content-Type", "video/mp4")
        .header("Cache-Control", "private, no-store");
    if request.method() == "HEAD" {
        return response
            .header("Content-Length", length)
            .body(Vec::new())
            .unwrap();
    }
    let (start, end) = if let Some(range) = request.headers().get("Range") {
        let Some(range) = range
            .to_str()
            .ok()
            .and_then(|value| byte_range(value, length))
        else {
            return response
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("Content-Range", format!("bytes */{length}"))
                .body(Vec::new())
                .unwrap();
        };
        response = response.status(StatusCode::PARTIAL_CONTENT).header(
            "Content-Range",
            format!("bytes {}-{}/{length}", range.0, range.1),
        );
        range
    } else if length <= MAX_CHUNK {
        (0, length - 1)
    } else {
        // WebView's media loader uses ranges. Do not let an unbounded fetch copy
        // a long recording into memory in one response.
        return error(StatusCode::BAD_REQUEST);
    };
    let size = end - start + 1;
    let mut bytes = vec![0; size as usize];
    if file
        .seek(SeekFrom::Start(start))
        .and_then(|_| file.read_exact(&mut bytes))
        .is_err()
    {
        return error(StatusCode::INTERNAL_SERVER_ERROR);
    }
    response.header("Content-Length", size).body(bytes).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_ranges_handle_seek_suffix_and_bound_memory() {
        assert_eq!(byte_range("bytes=4-7", 10), Some((4, 7)));
        assert_eq!(byte_range("bytes=4-", 10), Some((4, 9)));
        assert_eq!(byte_range("bytes=-3", 10), Some((7, 9)));
        assert_eq!(
            byte_range("bytes=0-", MAX_CHUNK * 3),
            Some((0, MAX_CHUNK - 1))
        );
        for value in [
            "bytes=10-",
            "bytes=8-4",
            "bytes=-0",
            "bytes=0-1,4-5",
            "junk",
        ] {
            assert_eq!(byte_range(value, 10), None, "{value}");
        }
    }
}
