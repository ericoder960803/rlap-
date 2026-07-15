import socket
import struct
import time

def send_cmd(host, port, payload):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(2)
        s.connect((host, port))
        s.sendall(payload)
        return s.recv(1024)

def run_rlap_v2_test():
    host = '127.0.0.1'
    port = 6379
    
    print("--- 🚀 Rlap V2 功能測試 ---")

    # 1. 測試 SET (Addr 100, TTL 0, Val 12345)
    # Payload: [Op:0x03][Addr:100][TTL:0][Val:12345]
    payload_set = struct.pack(">BIIq", 0x03, 100, 0, 12345)
    res = send_cmd(host, port, payload_set)
    print(f"[SET] Addr 100 -> 12345: {res.decode().strip()}")

    # 2. 測試 GET
    payload_get = struct.pack(">BI", 0x02, 100)
    res = send_cmd(host, port, payload_get)
    print(f"[GET] Addr 100: {res.decode().strip()}")

    # 3. 測試 TTL (Addr 200, TTL 2s, Val 999)
    payload_set_ttl = struct.pack(">BIIq", 0x03, 200, 2, 999)
    send_cmd(host, port, payload_set_ttl)
    print(f"[SET TTL] Addr 200 -> 999 (2s TTL)")

    # 立即 GET
    res = send_cmd(host, port, payload_get := struct.pack(">BI", 0x02, 200))
    print(f"[GET] Addr 200 (Immediate): {res.decode().strip()}")

    # 等待 3 秒
    print("Waiting 3 seconds for TTL to expire...")
    time.sleep(3)
    res = send_cmd(host, port, payload_get)
    print(f"[GET] Addr 200 (After 3s): {res.decode().strip()}")

    # 4. 測試 DEL
    payload_del = struct.pack(">BI", 0x06, 100)
    res = send_cmd(host, port, payload_del)
    print(f"[DEL] Addr 100: {res.decode().strip()}")

    res = send_cmd(host, port, struct.pack(">BI", 0x02, 100))
    print(f"[GET] Addr 100 (After DEL): {res.decode().strip()}")

if __name__ == "__main__":
    run_rlap_v2_test()
