import json
import subprocess
import sys
import time

binary = sys.argv[1]

p = subprocess.Popen(
    [binary],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
)


def send(obj):
    p.stdin.write(json.dumps(obj) + "\n")
    p.stdin.flush()


def recv():
    line = p.stdout.readline()
    if not line:
        err = p.stderr.read()
        raise RuntimeError(f"no output from server, stderr:\n{err}")
    return json.loads(line)


try:
    send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
    init = recv()
    assert init["id"] == 1, init
    print("initialize ok:", init["result"]["serverInfo"])

    send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
    tools = recv()
    names = [t["name"] for t in tools["result"]["tools"]]
    assert "summarizeViaSampling" in names, names
    print("tools/list ok:", names)

    send({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "summarizeViaSampling", "arguments": {"text": "a long rambling essay about sqlite"}}
    })

    sampling_req = recv()
    assert sampling_req["method"] == "sampling/createMessage", sampling_req
    req_id = sampling_req["id"]
    print("sampling request seen:", sampling_req["params"]["messages"][0]["content"]["text"])

    time.sleep(0.05)
    send({
        "jsonrpc": "2.0", "id": req_id,
        "result": {"content": {"text": "sqlite is a small embedded database"}}
    })

    final = recv()
    assert final["id"] == 3, final
    assert final["result"]["content"][0]["text"] == "sqlite is a small embedded database", final
    print("final result ok:", final["result"])

    print("ALL CHECKS PASSED")
finally:
    p.stdin.close()
    p.terminate()
    try:
        p.wait(timeout=2)
    except subprocess.TimeoutExpired:
        p.kill()
