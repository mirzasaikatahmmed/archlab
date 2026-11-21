//! Fixed-size frames over TCP. Requests and responses are 16 bytes and carry the
//! request id, so many requests can share one connection.

use tokio::io::{AsyncRead, AsyncReadExt};

pub const FRAME_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    pub read: bool,
    /// A cache already missed for this request, so it must not probe one again.
    pub cache_done: bool,
    /// Time the caller is still willing to wait, in microseconds.
    pub budget_us: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok = 0,
    /// Answered from a cache.
    Hit = 1,
    /// A cache missed and has nothing further to ask; the caller continues.
    Miss = 2,
    Rejected = 3,
    TimedOut = 4,
    Failed = 5,
}

impl Status {
    pub fn is_success(self) -> bool {
        matches!(self, Self::Ok | Self::Hit)
    }

    fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Ok,
            1 => Self::Hit,
            2 => Self::Miss,
            3 => Self::Rejected,
            4 => Self::TimedOut,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Response {
    pub id: u64,
    pub status: Status,
}

pub fn encode_request(r: &Request) -> [u8; FRAME_LEN] {
    let mut f = [0u8; FRAME_LEN];
    f[..8].copy_from_slice(&r.id.to_be_bytes());
    f[8] = u8::from(r.read) | (u8::from(r.cache_done) << 1);
    f[9..13].copy_from_slice(&r.budget_us.to_be_bytes());
    f
}

pub fn decode_request(f: &[u8; FRAME_LEN]) -> Request {
    Request {
        id: u64::from_be_bytes([f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7]]),
        read: f[8] & 1 != 0,
        cache_done: f[8] & 2 != 0,
        budget_us: u32::from_be_bytes([f[9], f[10], f[11], f[12]]),
    }
}

pub fn encode_response(r: &Response) -> [u8; FRAME_LEN] {
    let mut f = [0u8; FRAME_LEN];
    f[..8].copy_from_slice(&r.id.to_be_bytes());
    f[8] = r.status as u8;
    f
}

pub fn decode_response(f: &[u8; FRAME_LEN]) -> Response {
    Response {
        id: u64::from_be_bytes([f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7]]),
        status: Status::from_byte(f[8]),
    }
}

pub async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<[u8; FRAME_LEN]> {
    let mut frame = [0u8; FRAME_LEN];
    reader.read_exact(&mut frame).await?;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_for_every_flag_combination() {
        for (read, cache_done) in [(false, false), (true, false), (false, true), (true, true)] {
            let r = Request {
                id: 0xDEAD_BEEF_0123_4567,
                read,
                cache_done,
                budget_us: 9_876_543,
            };
            assert_eq!(decode_request(&encode_request(&r)), r);
        }
    }

    #[test]
    fn responses_round_trip_for_every_status() {
        for status in [
            Status::Ok,
            Status::Hit,
            Status::Miss,
            Status::Rejected,
            Status::TimedOut,
            Status::Failed,
        ] {
            let r = Response {
                id: u64::MAX,
                status,
            };
            assert_eq!(decode_response(&encode_response(&r)), r);
        }
    }

    #[test]
    fn unknown_status_bytes_are_failures() {
        let mut f = encode_response(&Response {
            id: 1,
            status: Status::Ok,
        });
        f[8] = 200;
        assert_eq!(decode_response(&f).status, Status::Failed);
    }

    #[test]
    fn only_ok_and_hit_are_successes() {
        assert!(Status::Ok.is_success() && Status::Hit.is_success());
        assert!(!Status::Miss.is_success() && !Status::Rejected.is_success());
    }

    #[test]
    fn frames_are_read_from_a_stream() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        runtime.block_on(frames_are_read());
    }

    async fn frames_are_read() {
        let r = Request {
            id: 5,
            read: true,
            cache_done: false,
            budget_us: 1,
        };
        let bytes = encode_request(&r);
        let mut cursor = std::io::Cursor::new(bytes.to_vec());
        let frame = read_frame(&mut cursor).await.expect("frame");
        assert_eq!(decode_request(&frame), r);
        assert!(read_frame(&mut cursor).await.is_err());
    }
}
