import socket
import struct
import time

def run_rlap_test():
    host = '127.0.0.1'
    port = 6379
    
    # 這裡選一個在 Cold Zone 的地址 (假設 L1 是 768，總容量約 3500)
    target_addr = 1000 
    
    print(f"--- 🚀 Rlap 方案 B 正式協議測試 ---")
    print(f"目標地址: {target_addr} (RAM 區域)")

    try:
        for i in range(1, 15):
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.settimeout(2)
                s.connect((host, port))
                
                # 封包構造：
                # > 代表 Big-Endian
                # B 代表 unsigned char (1 byte, OpCode 0x02 為 GET)
                # I 代表 unsigned int (4 bytes, Address)
                payload = struct.pack(">BI", 0x02, target_addr)
                
                s.sendall(payload)
                data = s.recv(1024)
                print(f"[{i:02d}] 請求 Addr {target_addr} -> {data.decode().strip()}")
                
            time.sleep(0.1) # 模擬真實存取間隔
            
    except Exception as e:
        print(f"❌ 錯誤: {e}")

if __name__ == "__main__":
    run_rlap_test()