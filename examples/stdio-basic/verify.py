import json
import subprocess
import sys

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
    assert "reverseString" in names, names
    print("tools/list ok:", names)

    send({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "reverseString", "arguments": {"text": "hello world"}}
    })

    final = recv()
    assert final["id"] == 3, final
    text = final["result"]["content"][0]["text"]
    assert text == "dlrow olleh", final
    print("final result ok:", final["result"])

    print("ALL CHECKS PASSED")
finally:
    p.stdin.close()
    p.terminate()
    try:
        p.wait(timeout=2)
    except subprocess.TimeoutExpired:
        p.kill()
