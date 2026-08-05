//! RESP (REdis Serialization Protocol) parsing.
//! `redis-cli` sends every command as an *array of bulk strings*.
//! `GET foo` arrives on the wire as:
//!
//! ```text
//! *2\r\n$3\r\nGET\r\n$3\r\nfoo\r\n
//! ^^     ^^   ^^^     ^^   ^^^
//! |      |    |       |    - payload, exactly 3 bytes
//! |      |    |       - 2nd bulk string, 3 bytes long
//! |      |    - payload
//! |      - 1st bulk string, 3 bytes long
//! - array header: 2 elements follow
//! ```
pub type Command = Vec<Vec<u8>>;
pub fn parse(buf: &[u8]) -> Result<Option<(Command, usize)>, String> {
    let mut i = 0usize;

    match buf.first() {
        None => return Ok(None),
        Some(&b'*') => {} // * tag
        Some(&b) => return Err(format!("expected '*', got {:?}", b as char)),
    }
    i += 1;

    let crlf = match find_crlf(buf, i) {
        Some(idx) => idx,
        None => return Ok(None),
    };
    let count: usize = std::str::from_utf8(&buf[i..crlf])
        .map_err(|_| "invalid element count".to_string())?
        .parse()
        .map_err(|_| "invalid element count".to_string())?;

    i = crlf + 2;

    let mut args = Vec::new();

    for _ in 0..count {
        // '$' tag
        match buf.get(i) {
            None => return Ok(None),
            Some(&b'$') => {}
            Some(&b) => return Err(format!("expected '$', got {:?}", b as char)),
        }
        i += 1;

        // payload length
        let crlf = match find_crlf(buf, i) {
            Some(idx) => idx,
            None => return Ok(None),
        };

        let len: usize = std::str::from_utf8(&buf[i..crlf])
            .map_err(|_| "invalid bulk length".to_string())?
            .parse()
            .map_err(|_| "invalid bulk length".to_string())?;
        i = crlf + 2;

        // client controlled .. cap before reach slice index
        if len > 512 * 1024 * 1024 {
            return Err("invalid bulk length".to_string());
        }
        if buf.len() < i + len + 2 {
            return Ok(None);
        }

        args.push(buf[i..i + len].to_vec());
        i += len + 2;
    }

    Ok(Some((args, i)))
}

fn find_crlf(buf: &[u8], from: usize) -> Option<usize> {
    let rest = buf.get(from..)?;
    rest.windows(2)
        .position(|window| window == b"\r\n")
        .map(|off| from + off)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GET_FOO: &[u8] = b"*2\r\n$3\r\nGET\r\n$3\r\nfoo\r\n";

    #[test]
    fn parses_a_full_command() {
        let (args, n) = parse(GET_FOO).unwrap().unwrap();
        assert_eq!(args, vec![b"GET".to_vec(), b"foo".to_vec()]);
        assert_eq!(n, GET_FOO.len());
    }

    #[test]
    fn every_truncation_is_incomplete() {
        for cut in 0..GET_FOO.len() {
            assert_eq!(parse(&GET_FOO[..cut]), Ok(None), "prefix of {cut} bytes");
        }
    }

    #[test]
    fn two_commands_back_to_back() {
        let mut both = GET_FOO.to_vec();
        both.extend_from_slice(GET_FOO);

        let (_, n) = parse(&both).unwrap().unwrap();
        assert_eq!(n, GET_FOO.len());

        let (args, n2) = parse(&both[n..]).unwrap().unwrap();
        assert_eq!(args, vec![b"GET".to_vec(), b"foo".to_vec()]);
        assert_eq!(n + n2, both.len());
    }

    #[test]
    fn values_may_contain_spaces() {
        let cmd = b"*3\r\n$3\r\nSET\r\n$1\r\na\r\n$5\r\nb c d\r\n";
        let (args, _) = parse(cmd).unwrap().unwrap();
        assert_eq!(args[2], b"b c d".to_vec());
    }

    #[test]
    fn values_may_contain_crlf() {
        let cmd = b"*3\r\n$3\r\nSET\r\n$1\r\na\r\n$4\r\nx\r\ny\r\n";
        let (args, n) = parse(cmd).unwrap().unwrap();
        assert_eq!(args[2], b"x\r\ny".to_vec());
        assert_eq!(n, cmd.len());
    }

    #[test]
    fn rejects_non_array() {
        assert!(parse(b"PING\r\n").is_err());
    }

    #[test]
    fn rejects_oversized_bulk_length() {
        assert!(parse(b"*1\r\n$600000000\r\n").is_err());
    }

    #[test]
    fn rejects_non_numeric_length() {
        assert!(parse(b"*1\r\n$abc\r\n").is_err());
    }
}
