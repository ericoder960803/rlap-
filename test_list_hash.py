import socket
import struct
import time

def send_cmd(host, port, payload):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(2)
        s.connect((host, port))
        s.sendall(payload)
        return s.recv(1024)

def run_list_hash_test():
    host = '127.0.0.1'
    port = 6379
    
    print("--- 🚀 Rlap List & Hash 功能測試 ---")

    # 1. 測試 List (LPush 3 次)
    print("\n[List 測試]")
    for val in [10, 20, 30]:
        # Op: 0x09 (LPush), Addr: 300, Val: val
        payload = struct.pack(">BIq", 0x09, 300, val)
        send_cmd(host, port, payload)
        print(f"LPush {val} to Addr 300")

    # 2. 測試 LPop (彈出 2 次)
    for _ in range(2):
        payload = struct.pack(">BI", 0x0a, 300)
        res = send_cmd(host, port, payload)
        print(f"LPop from Addr 300: {res.decode().strip()}")

    # 3. 測試 Hash (HSet)
    print("\n[Hash 測試]")
    # Op: 0x0b (HSet), Addr: 400, FieldID: 1, Val: 8888
    payload = struct.pack(">BIIq", 0x0b, 400, 1, 8888)
    send_cmd(host, port, payload)
    print("HSet Addr 400, Field 1 -> 8888")

    # 讀取整個 Hash
    payload_get = struct.pack(">BI", 0x02, 400)
    res = send_cmd(host, port, payload_get)
    print(f"GET Addr 400 (Hash): {res.decode().strip()}")

if __name__ == "__main__":
    run_list_hash_test()
