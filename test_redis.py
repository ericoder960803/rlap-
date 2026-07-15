import socket
import time

def send_resp_cmd(host, port, args):
    resp = f"*{len(args)}\r\n"
    for arg in args:
        arg_str = str(arg)
        resp += f"${len(arg_str.encode())}\r\n{arg_str}\r\n"
    
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(2)
        s.connect((host, port))
        s.sendall(resp.encode())
        return s.recv(4096)

def run_redis_test():
    host = '127.0.0.1'
    port = 6379
    
    print("--- 🚀 Rlap Redis 協議功能測試 ---")

    # 1. PING
    print("\n[PING 測試]")
    res = send_resp_cmd(host, port, ["PING"])
    print(f"PING -> {res.decode().strip()}")

    # 2. SET & GET
    print("\n[SET & GET 測試]")
    res = send_resp_cmd(host, port, ["SET", "mykey", "hello_rlap"])
    print(f"SET mykey hello_rlap -> {res.decode().strip()}")
    
    res = send_resp_cmd(host, port, ["GET", "mykey"])
    print(f"GET mykey -> {res.decode().strip()}")

    # 3. SET (Integer) & GET
    res = send_resp_cmd(host, port, ["SET", "num", "12345"])
    print(f"SET num 12345 -> {res.decode().strip()}")
    
    res = send_resp_cmd(host, port, ["GET", "num"])
    print(f"GET num -> {res.decode().strip()}")

    # 4. LPUSH & LPOP
    print("\n[List 測試]")
    send_resp_cmd(host, port, ["LPUSH", "mylist", "first"])
    send_resp_cmd(host, port, ["LPUSH", "mylist", "second"])
    print("LPUSH mylist first, second")
    
    res = send_resp_cmd(host, port, ["LPOP", "mylist"])
    print(f"LPOP mylist -> {res.decode().strip()}")
    res = send_resp_cmd(host, port, ["LPOP", "mylist"])
    print(f"LPOP mylist -> {res.decode().strip()}")

    # 5. DEL
    print("\n[DEL 測試]")
    send_resp_cmd(host, port, ["SET", "todelete", "val"])
    res = send_resp_cmd(host, port, ["DEL", "todelete"])
    print(f"DEL todelete -> {res.decode().strip()}")
    res = send_resp_cmd(host, port, ["GET", "todelete"])
    print(f"GET todelete -> {res.decode().strip()}")

    # 6. TTL & EXPIRE
    print("\n[TTL & EXPIRE 測試]")
    send_resp_cmd(host, port, ["SET", "ttlkey", "val", "EX", "10"])
    res = send_resp_cmd(host, port, ["TTL", "ttlkey"])
    print(f"SET ttlkey val EX 10 -> TTL: {res.decode().strip()}")
    
    send_resp_cmd(host, port, ["SET", "expirekey", "val"])
    send_resp_cmd(host, port, ["EXPIRE", "expirekey", "5"])
    res = send_resp_cmd(host, port, ["TTL", "expirekey"])
    print(f"EXPIRE expirekey 5 -> TTL: {res.decode().strip()}")

    # 7. Hash 測試
    print("\n[Hash 測試]")
    send_resp_cmd(host, port, ["HSET", "myhash", "f1", "v1"])
    send_resp_cmd(host, port, ["HSET", "myhash", "f2", "100"])
    print("HSET myhash f1 v1, f2 100")
    
    res = send_resp_cmd(host, port, ["HGET", "myhash", "f1"])
    print(f"HGET myhash f1 -> {res.decode().strip()}")
    res = send_resp_cmd(host, port, ["HGET", "myhash", "f2"])
    print(f"HGET myhash f2 -> {res.decode().strip()}")

    res = send_resp_cmd(host, port, ["HGETALL", "myhash"])
    print(f"HGETALL myhash -> {res.decode().strip()}")

if __name__ == "__main__":
    run_redis_test()
