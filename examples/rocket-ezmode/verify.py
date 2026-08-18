import json
import subprocess
import sys
import time
import urllib.request

binary = sys.argv[1]
url = "http://127.0.0.1:8000/mcp"

p = subprocess.Popen([binary], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def call(obj):
    req = urllib.request.Request(
        url,
        data=json.dumps(obj).encode(),
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=5) as resp:
        return json.loads(resp.read())


def call_sse(obj):
    """POSTs a tools/call and reads back raw `data:` lines as they arrive off the SSE
    response, rather than waiting for the whole body -- urlopen().read() would block
    until the connection closes, which defeats the point of verifying it streams."""
    req = urllib.request.Request(
        url,
        data=json.dumps(obj).encode(),
        headers={"content-type": "application/json"},
        method="POST",
    )
    events = []
    with urllib.request.urlopen(req, timeout=5) as resp:
        for raw_line in resp:
            line = raw_line.decode().strip()
            if line.startswith("data:"):
                events.append(json.loads(line[len("data:") :]))
    return events


try:
    for _ in range(50):
        try:
            urllib.request.urlopen(url, timeout=0.2)
            break
        except Exception:
            time.sleep(0.1)

    init = call({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
    assert init["id"] == 1, init
    print("initialize ok:", init["result"]["serverInfo"])

    templates = call({"jsonrpc": "2.0", "id": 2, "method": "resources/templates/list", "params": {}})
    uris = [t["uriTemplate"] for t in templates["result"]["resourceTemplates"]]
    assert "book://{id}" in uris, uris
    print("resources/templates/list ok:", uris)

    book = call({"jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": {"uri": "book://1"}})
    text = book["result"]["contents"][0]["text"]
    assert "The C Programming Language" in text, text
    print("resources/read book://1 ok:", text.splitlines()[0])

    tools = call({"jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": {}})
    names = [t["name"] for t in tools["result"]["tools"]]
    assert "searchBooks" in names, names
    print("tools/list ok:", names)

    search = call({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": {"name": "searchBooks", "arguments": {"query": "Gibson"}}
    })
    hit = search["result"]["content"][0]["text"]
    assert "Neuromancer" in hit, hit
    print("tools/call searchBooks ok:", hit.splitlines()[0])

    events = call_sse({
        "jsonrpc": "2.0", "id": 6, "method": "tools/call",
        "params": {"name": "countTo", "arguments": {"n": 4}}
    })
    progress = [e["params"]["progress"] for e in events if e.get("method") == "notifications/progress"]
    assert progress == [1, 2, 3, 4], progress
    final = events[-1]
    assert final["id"] == 6 and final["result"]["countedTo"] == 4, final
    print("tools/call countTo (SSE) ok:", progress, "->", final["result"])

    print("ALL CHECKS PASSED")
finally:
    p.terminate()
    try:
        p.wait(timeout=2)
    except subprocess.TimeoutExpired:
        p.kill()
