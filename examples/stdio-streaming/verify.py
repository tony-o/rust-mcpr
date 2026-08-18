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
    assert "countTo" in names, names
    print("tools/list ok:", names)

    # plain tool still works unchanged alongside the streaming one
    send({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "reverseString", "arguments": {"text": "streaming"}}
    })
    reverse_result = recv()
    assert reverse_result["id"] == 3, reverse_result
    text = reverse_result["result"]["content"][0]["text"]
    assert text == "gnimaerts", reverse_result
    print("reverseString ok:", reverse_result["result"])

    # streaming tool: progress notifications, then the final id-wrapped result
    send({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "countTo", "arguments": {"n": 4}}
    })

    progress_seen = []
    while True:
        item = recv()
        if item.get("method") == "notifications/progress":
            progress_seen.append(item["params"]["progress"])
            continue
        final = item
        break

    assert progress_seen == [1, 2, 3, 4], progress_seen
    print("progress notifications ok:", progress_seen)

    assert final["id"] == 4, final
    assert final["result"] == {"countedTo": 4}, final
    print("final result ok:", final["result"])

    print("ALL CHECKS PASSED")
finally:
    p.stdin.close()
    p.terminate()
    try:
        p.wait(timeout=2)
    except subprocess.TimeoutExpired:
        p.kill()
