use crate::protocol::RlapValue;
use aligned_vec::AVec;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, Duration};

#[repr(C, align(64))]
pub struct Slot {
    pub data: RlapValue,
    pub access_count: AtomicU64,
    pub pinned: bool,
    pub expires_at: Option<Instant>, // TTL 支援
}

pub struct RlapDb {
    pub hot_zone: AVec<Slot>,
    pub cold_zone: Vec<Slot>,
    pub stack: Vec<RlapValue>,
}

impl RlapDb {
    pub fn new(hot_cap: usize, cold_cap: usize) -> Self {
        let mut hot = AVec::new(64); 
        for _ in 0..hot_cap { 
            hot.push(Slot { data: RlapValue::Empty, access_count: AtomicU64::new(0), pinned: false, expires_at: None }); 
        }
        let mut cold = Vec::with_capacity(cold_cap);
        for _ in 0..cold_cap { 
            cold.push(Slot { data: RlapValue::Empty, access_count: AtomicU64::new(0), pinned: false, expires_at: None }); 
        }
        Self { hot_zone: hot, cold_zone: cold, stack: Vec::with_capacity(64) }
    }

    fn get_mut_slot(&mut self, addr: usize) -> Option<&mut Slot> {
        let hot_len = self.hot_zone.len();
        let is_hot = addr < hot_len;
        let slot = if is_hot {
            &mut self.hot_zone[addr]
        } else {
            let c_idx = addr - hot_len;
            if c_idx >= self.cold_zone.len() { return None; }
            &mut self.cold_zone[c_idx]
        };

        // 檢查 TTL (Lazy Eviction)
        if let Some(expiry) = slot.expires_at {
            if Instant::now() > expiry {
                slot.data = RlapValue::Empty;
                slot.expires_at = None;
                slot.access_count.store(0, Ordering::SeqCst);
            }
        }
        Some(slot)
    }

    fn try_promote(&mut self, addr: usize) {
        let hot_len = self.hot_zone.len();
        if addr < hot_len { return; } // 已經在 Hot Zone
        
        let c_idx = addr - hot_len;
        if c_idx >= self.cold_zone.len() { return; }

        // 原子加法並獲取先前計數
        let prev_count = self.cold_zone[c_idx].access_count.fetch_add(1, Ordering::SeqCst);

        if prev_count == 5 && !self.cold_zone[c_idx].pinned {
            let h_idx = addr % hot_len;
            if !self.hot_zone[h_idx].pinned {
                let cold_val = std::mem::replace(&mut self.cold_zone[c_idx].data, RlapValue::Empty);
                let old_hot_val = std::mem::replace(&mut self.hot_zone[h_idx].data, cold_val);
                self.cold_zone[c_idx].data = old_hot_val;

                let cold_ttl = self.cold_zone[c_idx].expires_at;
                self.cold_zone[c_idx].expires_at = self.hot_zone[h_idx].expires_at;
                self.hot_zone[h_idx].expires_at = cold_ttl;

                println!("🚀 [Promotion] Addr {} -> Hot Slot {}", addr, h_idx);
            }
        }
    }

    pub fn set(&mut self, addr: usize, val: RlapValue, ttl_secs: Option<u64>) {
        if let Some(slot) = self.get_mut_slot(addr) {
            slot.data = val;
            slot.expires_at = ttl_secs.map(|s| Instant::now() + Duration::from_secs(s));
            slot.access_count.store(0, Ordering::SeqCst);
        }
    }

    pub fn del(&mut self, addr: usize) {
        if let Some(slot) = self.get_mut_slot(addr) {
            slot.data = RlapValue::Empty;
            slot.expires_at = None;
            slot.access_count.store(0, Ordering::SeqCst);
        }
    }

    pub fn get_and_promote(&mut self, addr: usize) -> RlapValue {
        let hot_len = self.hot_zone.len();
        
        let val = if let Some(slot) = self.get_mut_slot(addr) {
            slot.data.clone()
        } else {
            return RlapValue::Empty;
        };

        if val == RlapValue::Empty {
            return RlapValue::Empty;
        }

        if addr < hot_len {
            if let Some(slot) = self.get_mut_slot(addr) {
                slot.access_count.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.try_promote(addr);
        }

        val
    }

    pub fn push_list(&mut self, addr: usize, val: RlapValue) {
        if let Some(slot) = self.get_mut_slot(addr) {
            if let RlapValue::List(ref mut list) = slot.data {
                list.push_front(val); // LPUSH 應該是 push_front
            } else {
                let mut new_list = std::collections::VecDeque::new();
                new_list.push_front(val);
                slot.data = RlapValue::List(new_list);
            }
        }
        self.try_promote(addr);
    }

    pub fn pop_list(&mut self, addr: usize) -> RlapValue {
        let val = if let Some(slot) = self.get_mut_slot(addr) {
            if let RlapValue::List(ref mut list) = slot.data {
                list.pop_front().unwrap_or(RlapValue::Empty)
            } else {
                RlapValue::Empty
            }
        } else {
            RlapValue::Empty
        };
        self.try_promote(addr);
        val
    }

    pub fn set_hash(&mut self, addr: usize, field: String, val: RlapValue) {
        if let Some(slot) = self.get_mut_slot(addr) {
            if let RlapValue::Hash(ref mut map) = slot.data {
                map.insert(field, val);
            } else {
                let mut new_map = std::collections::HashMap::new();
                new_map.insert(field, val);
                slot.data = RlapValue::Hash(new_map);
            }
        }
        self.try_promote(addr);
    }

    pub fn get_hash(&mut self, addr: usize, field: &str) -> RlapValue {
        let val = if let Some(slot) = self.get_mut_slot(addr) {
            if let RlapValue::Hash(ref mut map) = slot.data {
                map.get(field).cloned().unwrap_or(RlapValue::Empty)
            } else {
                RlapValue::Empty
            }
        } else {
            RlapValue::Empty
        };
        self.try_promote(addr);
        val
    }

    pub fn get_hash_all(&mut self, addr: usize) -> Vec<(String, RlapValue)> {
        let res = if let Some(slot) = self.get_mut_slot(addr) {
            if let RlapValue::Hash(ref map) = slot.data {
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        self.try_promote(addr);
        res
    }

    pub fn expire(&mut self, addr: usize, ttl_secs: u64) -> bool {
        if let Some(slot) = self.get_mut_slot(addr) {
            if slot.data != RlapValue::Empty {
                slot.expires_at = Some(Instant::now() + Duration::from_secs(ttl_secs));
                return true;
            }
        }
        false
    }

    pub fn ttl(&mut self, addr: usize) -> i64 {
        if let Some(slot) = self.get_mut_slot(addr) {
            if slot.data == RlapValue::Empty {
                return -2; // Key 不存在
            }
            if let Some(expiry) = slot.expires_at {
                let now = Instant::now();
                if expiry > now {
                    return (expiry - now).as_secs() as i64;
                } else {
                    return -2; // 已過期
                }
            }
            return -1; // 沒有設置過期時間
        }
        -2
    }
}
