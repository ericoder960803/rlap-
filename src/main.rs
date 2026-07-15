mod core;
mod protocol;
mod net;

use tokio::io::AsyncReadExt;
use std::path::Path;

async fn load_aof(db: &mut core::RlapDb) -> tokio::io::Result<()> {
    if !Path::new("rlap.aof").exists() {
        return Ok(());
    }

    let mut file = tokio::fs::File::open("rlap.aof").await?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await?;

    let mut pos = 0;
    while pos < buf.len() {
        let op = buf[pos];
        match protocol::OpCode::from_u8(op) {
            Some(protocol::OpCode::Set) => {
                if pos + 17 <= buf.len() {
                    let addr = u32::from_be_bytes([buf[pos+1], buf[pos+2], buf[pos+3], buf[pos+4]]) as usize;
                    let ttl = u32::from_be_bytes([buf[pos+5], buf[pos+6], buf[pos+7], buf[pos+8]]) as u64;
                    let val = i64::from_be_bytes([buf[pos+9], buf[pos+10], buf[pos+11], buf[pos+12], buf[pos+13], buf[pos+14], buf[pos+15], buf[pos+16]]);
                    let ttl_opt = if ttl > 0 { Some(ttl) } else { None };
                    db.set(addr, protocol::RlapValue::Integer(val), ttl_opt);
                    pos += 17;
                } else { break; }
            }
            Some(protocol::OpCode::Del) => {
                if pos + 5 <= buf.len() {
                    let addr = u32::from_be_bytes([buf[pos+1], buf[pos+2], buf[pos+3], buf[pos+4]]) as usize;
                    db.del(addr);
                    pos += 5;
                } else { break; }
            }
            Some(protocol::OpCode::LPush) => {
                if pos + 13 <= buf.len() {
                    let addr = u32::from_be_bytes([buf[pos+1], buf[pos+2], buf[pos+3], buf[pos+4]]) as usize;
                    let val = i64::from_be_bytes([buf[pos+5], buf[pos+6], buf[pos+7], buf[pos+8], buf[pos+9], buf[pos+10], buf[pos+11], buf[pos+12]]);
                    db.push_list(addr, protocol::RlapValue::Integer(val));
                    pos += 13;
                } else { break; }
            }
            Some(protocol::OpCode::LPop) => {
                if pos + 5 <= buf.len() {
                    let addr = u32::from_be_bytes([buf[pos+1], buf[pos+2], buf[pos+3], buf[pos+4]]) as usize;
                    db.pop_list(addr);
                    pos += 5;
                } else { break; }
            }
            Some(protocol::OpCode::HSet) => {
                if pos + 17 <= buf.len() {
                    let addr = u32::from_be_bytes([buf[pos+1], buf[pos+2], buf[pos+3], buf[pos+4]]) as usize;
                    let field_id = u32::from_be_bytes([buf[pos+5], buf[pos+6], buf[pos+7], buf[pos+8]]);
                    let val = i64::from_be_bytes([buf[pos+9], buf[pos+10], buf[pos+11], buf[pos+12], buf[pos+13], buf[pos+14], buf[pos+15], buf[pos+16]]);
                    db.set_hash(addr, field_id.to_string(), protocol::RlapValue::Integer(val));
                    pos += 17;
                } else { break; }
            }
            _ => pos += 1, // 跳過未知或非寫入指令
        }
    }
    println!("✅ AOF 恢復完成");
    Ok(())
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let l1_size = cache_size::l1_cache_size().unwrap_or(32768);
    let hot_slots = l1_size / 64; 
    let mut db = core::RlapDb::new(hot_slots, 3000);

    // 啟動前先從 AOF 恢復
    load_aof(&mut db).await?;

    println!("--- 🛠️ Rlap 企業級資料庫系統 ---");

    net::start_server("127.0.0.1:6379", db).await?;

    Ok(())
}