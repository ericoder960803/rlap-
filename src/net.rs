use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::core::RlapDb;
use crate::protocol::{OpCode, RespValue, RlapValue};
use std::sync::Arc;
use parking_lot::RwLock; 
use tokio::fs::OpenOptions;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn key_to_addr(key: &str, hot_len: usize, cold_len: usize) -> usize {
    let mut s = DefaultHasher::new();
    key.hash(&mut s);
    let total = hot_len + cold_len;
    (s.finish() as usize) % total
}

pub async fn start_server(addr: &str, db: RlapDb) -> tokio::io::Result<()> {
    let hot_len = db.hot_zone.len();
    let cold_len = db.cold_zone.len();
    let shared_db = Arc::new(RwLock::new(db));
    let listener = TcpListener::bind(addr).await?;
    println!("Rlap 多執行緒伺服器已啟動: {}", addr);

    let aof_file = Arc::new(tokio::sync::Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open("rlap.aof")
            .await?
    ));

    loop {
        let (mut socket, _) = listener.accept().await?;
        let db_ptr = Arc::clone(&shared_db);
        let aof_ptr = Arc::clone(&aof_file);

        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };

                if n == 0 { break; }

                // 判斷是否為 RESP 協議 (以 * 開頭)
                if buf[0] == b'*' {
                    if let Some((RespValue::Array(args), _)) = RespValue::parse(&buf[..n]) {
                        if args.is_empty() { continue; }
                        let cmd_name = match &args[0] {
                            RespValue::BulkString(s) => String::from_utf8_lossy(s).to_uppercase(),
                            _ => continue,
                        };

                        match cmd_name.as_str() {
                            "PING" => {
                                let _ = socket.write_all(&RespValue::SimpleString("PONG".to_string()).to_bytes()).await;
                            }
                            "SET" => {
                                if args.len() >= 3 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let val_bytes = match &args[2] {
                                        RespValue::BulkString(s) => s.clone(),
                                        _ => vec![],
                                    };
                                    
                                    let mut ttl_opt = None;
                                    if args.len() >= 5 {
                                        if let RespValue::BulkString(opt) = &args[3] {
                                            if String::from_utf8_lossy(opt).to_uppercase() == "EX" {
                                                if let RespValue::BulkString(secs_bytes) = &args[4] {
                                                    ttl_opt = String::from_utf8_lossy(secs_bytes).parse::<u64>().ok();
                                                }
                                            }
                                        }
                                    }

                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    
                                    let r_val = if let Ok(i) = String::from_utf8_lossy(&val_bytes).parse::<i64>() {
                                        RlapValue::Integer(i)
                                    } else {
                                        RlapValue::LongStr(String::from_utf8_lossy(&val_bytes).into_owned())
                                    };

                                    {
                                        let mut db = db_ptr.write();
                                        db.set(addr, r_val, ttl_opt);
                                    }
                                    let _ = socket.write_all(&RespValue::SimpleString("OK".to_string()).to_bytes()).await;
                                }
                            }
                            "GET" => {
                                if args.len() >= 2 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let val = {
                                        let mut db = db_ptr.write();
                                        db.get_and_promote(addr)
                                    };
                                    
                                    let resp = match val {
                                        RlapValue::Integer(i) => RespValue::BulkString(i.to_string().into_bytes()),
                                        RlapValue::LongStr(s) => RespValue::BulkString(s.into_bytes()),
                                        RlapValue::Empty => RespValue::Nil,
                                        _ => RespValue::SimpleString(format!("{:?}", val)),
                                    };
                                    let _ = socket.write_all(&resp.to_bytes()).await;
                                }
                            }
                            "DEL" => {
                                if args.len() >= 2 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    {
                                        let mut db = db_ptr.write();
                                        db.del(addr);
                                    }
                                    let _ = socket.write_all(&RespValue::Integer(1).to_bytes()).await;
                                }
                            }
                            "EXPIRE" => {
                                if args.len() >= 3 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let secs = match &args[2] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s).parse::<u64>().unwrap_or(0),
                                        _ => 0,
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let ok = {
                                        let mut db = db_ptr.write();
                                        db.expire(addr, secs)
                                    };
                                    let _ = socket.write_all(&RespValue::Integer(if ok { 1 } else { 0 }).to_bytes()).await;
                                }
                            }
                            "TTL" => {
                                if args.len() >= 2 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let t = {
                                        let mut db = db_ptr.write();
                                        db.ttl(addr)
                                    };
                                    let _ = socket.write_all(&RespValue::Integer(t).to_bytes()).await;
                                }
                            }
                            "HSET" => {
                                if args.len() >= 4 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let field = match &args[2] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s).into_owned(),
                                        _ => "".into(),
                                    };
                                    let val_bytes = match &args[3] {
                                        RespValue::BulkString(s) => s.clone(),
                                        _ => vec![],
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let r_val = if let Ok(i) = String::from_utf8_lossy(&val_bytes).parse::<i64>() {
                                        RlapValue::Integer(i)
                                    } else {
                                        RlapValue::LongStr(String::from_utf8_lossy(&val_bytes).into_owned())
                                    };
                                    {
                                        let mut db = db_ptr.write();
                                        db.set_hash(addr, field, r_val);
                                    }
                                    let _ = socket.write_all(&RespValue::Integer(1).to_bytes()).await;
                                }
                            }
                            "HGET" => {
                                if args.len() >= 3 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let field = match &args[2] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let val = {
                                        let mut db = db_ptr.write();
                                        db.get_hash(addr, &field)
                                    };
                                    let resp = match val {
                                        RlapValue::Integer(i) => RespValue::BulkString(i.to_string().into_bytes()),
                                        RlapValue::LongStr(s) => RespValue::BulkString(s.into_bytes()),
                                        RlapValue::Empty => RespValue::Nil,
                                        _ => RespValue::SimpleString(format!("{:?}", val)),
                                    };
                                    let _ = socket.write_all(&resp.to_bytes()).await;
                                }
                            }
                            "HGETALL" => {
                                if args.len() >= 2 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let fields = {
                                        let mut db = db_ptr.write();
                                        db.get_hash_all(addr)
                                    };
                                    let mut resp_items = Vec::with_capacity(fields.len() * 2);
                                    for (f, v) in fields {
                                        resp_items.push(RespValue::BulkString(f.into_bytes()));
                                        let v_resp = match v {
                                            RlapValue::Integer(i) => RespValue::BulkString(i.to_string().into_bytes()),
                                            RlapValue::LongStr(s) => RespValue::BulkString(s.into_bytes()),
                                            _ => RespValue::Nil,
                                        };
                                        resp_items.push(v_resp);
                                    }
                                    let _ = socket.write_all(&RespValue::Array(resp_items).to_bytes()).await;
                                }
                            }
                            "LPUSH" => {
                                if args.len() >= 3 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let val_bytes = match &args[2] {
                                        RespValue::BulkString(s) => s.clone(),
                                        _ => vec![],
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let r_val = if let Ok(i) = String::from_utf8_lossy(&val_bytes).parse::<i64>() {
                                        RlapValue::Integer(i)
                                    } else {
                                        RlapValue::LongStr(String::from_utf8_lossy(&val_bytes).into_owned())
                                    };
                                    {
                                        let mut db = db_ptr.write();
                                        db.push_list(addr, r_val);
                                    }
                                    let _ = socket.write_all(&RespValue::Integer(1).to_bytes()).await;
                                }
                            }
                            "LPOP" => {
                                if args.len() >= 2 {
                                    let key = match &args[1] {
                                        RespValue::BulkString(s) => String::from_utf8_lossy(s),
                                        _ => "".into(),
                                    };
                                    let addr = key_to_addr(&key, hot_len, cold_len);
                                    let val = {
                                        let mut db = db_ptr.write();
                                        db.pop_list(addr)
                                    };
                                    let resp = match val {
                                        RlapValue::Integer(i) => RespValue::BulkString(i.to_string().into_bytes()),
                                        RlapValue::LongStr(s) => RespValue::BulkString(s.into_bytes()),
                                        RlapValue::Empty => RespValue::Nil,
                                        _ => RespValue::SimpleString(format!("{:?}", val)),
                                    };
                                    let _ = socket.write_all(&resp.to_bytes()).await;
                                }
                            }
                            "COMMAND" => {
                                // 讓 redis-cli 以為我們是完整的 Redis
                                let _ = socket.write_all(&RespValue::Array(vec![]).to_bytes()).await;
                            }
                            _ => {
                                let _ = socket.write_all(&RespValue::Error(format!("Unknown command: {}", cmd_name)).to_bytes()).await;
                            }
                        }
                    }
                } else if n >= 5 {
                    // 原有的二進位協議邏輯
                    if let Some(cmd) = OpCode::from_u8(buf[0]) {
                        match cmd {
                            OpCode::Get => {
                                let addr_bytes = [buf[1], buf[2], buf[3], buf[4]];
                                let addr = u32::from_be_bytes(addr_bytes) as usize;

                                let val = {
                                    let mut db = db_ptr.write();
                                    db.get_and_promote(addr)
                                };

                                let _ = socket.write_all(format!("Result: {:?}\n", val).as_bytes()).await;
                            }
                            OpCode::Set => {
                                if n >= 17 {
                                    let addr = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                                    let ttl = u32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]) as u64;
                                    let val = i64::from_be_bytes([buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16]]);

                                    let ttl_opt = if ttl > 0 { Some(ttl) } else { None };

                                    {
                                        let mut db = db_ptr.write();
                                        db.set(addr, RlapValue::Integer(val), ttl_opt);
                                    }

                                    let mut f = aof_ptr.lock().await;
                                    let _ = f.write_all(&buf[..17]).await;

                                    let _ = socket.write_all(b"OK\n").await;
                                }
                            }
                            OpCode::LPush => {
                                if n >= 13 {
                                    let addr = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                                    let val = i64::from_be_bytes([buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12]]);
                                    {
                                        let mut db = db_ptr.write();
                                        db.push_list(addr, RlapValue::Integer(val));
                                    }
                                    let mut f = aof_ptr.lock().await;
                                    let _ = f.write_all(&buf[..13]).await;
                                    let _ = socket.write_all(b"OK\n").await;
                                }
                            }
                            OpCode::LPop => {
                                if n >= 5 {
                                    let addr = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                                    let val = {
                                        let mut db = db_ptr.write();
                                        db.pop_list(addr)
                                    };
                                    let mut f = aof_ptr.lock().await;
                                    let _ = f.write_all(&buf[..5]).await;
                                    let _ = socket.write_all(format!("Pop: {:?}\n", val).as_bytes()).await;
                                }
                            }
                            OpCode::HSet => {
                                if n >= 17 {
                                    let addr = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                                    let field_id = u32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]);
                                    let val = i64::from_be_bytes([buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16]]);
                                    {
                                        let mut db = db_ptr.write();
                                        db.set_hash(addr, field_id.to_string(), RlapValue::Integer(val));
                                    }
                                    let mut f = aof_ptr.lock().await;
                                    let _ = f.write_all(&buf[..17]).await;
                                    let _ = socket.write_all(b"OK\n").await;
                                }
                            }
                            OpCode::Del => {
                                if n >= 5 {
                                    let addr = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                                    {
                                        let mut db = db_ptr.write();
                                        db.del(addr);
                                    }
                                    let mut f = aof_ptr.lock().await;
                                    let _ = f.write_all(&buf[..5]).await;
                                    let _ = socket.write_all(b"OK\n").await;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }
}