#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    Push    = 0x01,
    Get     = 0x02,
    Set     = 0x03,
    Calc    = 0x04,
    Migrate = 0x05,
    Del     = 0x06,
    Sync    = 0x07,
    Ret     = 0x08,
    LPush   = 0x09, // 新增：推入列表
    LPop    = 0x0a, // 新增：彈出列表
    HSet    = 0x0b, // 新增：設定雜湊欄位
    Halt    = 0xff, 
}

impl OpCode {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(OpCode::Push),
            0x02 => Some(OpCode::Get),
            0x03 => Some(OpCode::Set),
            0x04 => Some(OpCode::Calc),
            0x05 => Some(OpCode::Migrate),
            0x06 => Some(OpCode::Del),
            0x07 => Some(OpCode::Sync),
            0x08 => Some(OpCode::Ret),
            0x09 => Some(OpCode::LPush),
            0x0a => Some(OpCode::LPop),
            0x0b => Some(OpCode::HSet),
            0xff => Some(OpCode::Halt),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum CalcOp {
    Add = 0x01, Sub = 0x02, Max = 0x03, Min = 0x04,
}

use std::collections::{VecDeque, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub enum RlapValue {
    Empty,
    Integer(i64),
    ShortStr([u8; 32], usize), 
    LongStr(String),
    List(VecDeque<RlapValue>),
    Hash(HashMap<String, RlapValue>),
}

// --- RESP 協議支援 ---

#[derive(Debug, Clone, PartialEq)]
pub enum RespValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Vec<u8>),
    Array(Vec<RespValue>),
    Nil,
}

impl RespValue {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            RespValue::SimpleString(s) => format!("+{}\r\n", s).into_bytes(),
            RespValue::Error(s) => format!("-{}\r\n", s).into_bytes(),
            RespValue::Integer(i) => format!(":{}\r\n", i).into_bytes(),
            RespValue::BulkString(b) => {
                let mut res = format!("${}\r\n", b.len()).into_bytes();
                res.extend_from_slice(b);
                res.extend_from_slice(b"\r\n");
                res
            }
            RespValue::Array(a) => {
                let mut res = format!("*{}\r\n", a.len()).into_bytes();
                for item in a {
                    res.extend_from_slice(&item.to_bytes());
                }
                res
            }
            RespValue::Nil => b"$-1\r\n".to_vec(),
        }
    }

    pub fn parse(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.is_empty() { return None; }
        match buf[0] {
            b'+' => {
                let (line, len) = read_line(&buf[1..])?;
                Some((RespValue::SimpleString(String::from_utf8_lossy(line).into_owned()), len + 1))
            }
            b'-' => {
                let (line, len) = read_line(&buf[1..])?;
                Some((RespValue::Error(String::from_utf8_lossy(line).into_owned()), len + 1))
            }
            b':' => {
                let (line, len) = read_line(&buf[1..])?;
                let val = String::from_utf8_lossy(line).parse::<i64>().ok()?;
                Some((RespValue::Integer(val), len + 1))
            }
            b'$' => {
                let (line, len) = read_line(&buf[1..])?;
                let size = String::from_utf8_lossy(line).parse::<isize>().ok()?;
                if size == -1 {
                    return Some((RespValue::Nil, len + 1));
                }
                let size = size as usize;
                let start = len + 1;
                if buf.len() < start + size + 2 { return None; }
                let data = buf[start..start + size].to_vec();
                Some((RespValue::BulkString(data), start + size + 2))
            }
            b'*' => {
                let (line, len) = read_line(&buf[1..])?;
                let count = String::from_utf8_lossy(line).parse::<isize>().ok()?;
                if count == -1 {
                    return Some((RespValue::Nil, len + 1));
                }
                let count = count as usize;
                let mut pos = len + 1;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    let (item, item_len) = RespValue::parse(&buf[pos..])?;
                    items.push(item);
                    pos += item_len;
                }
                Some((RespValue::Array(items), pos))
            }
            _ => None,
        }
    }
}

fn read_line(buf: &[u8]) -> Option<(&[u8], usize)> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\r' && buf[i+1] == b'\n' {
            return Some((&buf[..i], i + 2));
        }
    }
    None
}