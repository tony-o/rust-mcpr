import json
import subprocess
import sys
import time
import urllib.request

binary = sys.argv[1]
url = "http://127.0.0.1:3000/mcp"

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


def call_streaming(obj):
    # The connection stays open for the life of the stream, so read line-by-line
    # instead of resp.read() (which would block until the server closes it).
    req = urllib.request.Request(
        url,
        data=json.dumps(obj).encode(),
        headers={"content-type": "application/json"},
        method="POST",
    )
    items = []
    with urllib.request.urlopen(req, timeout=5) as resp:
        for raw in resp:
            line = raw.decode().strip()
            if line.startswith("data:"):
                items.append(json.loads(line[len("data:") :].strip()))
    return items


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
    assert "countTo" in names, names
    print("tools/list ok:", names)

    search = call({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": {"name": "searchBooks", "arguments": {"query": "Gibson"}}
    })
    hit = search["result"]["content"][0]["text"]
    assert "Neuromancer" in hit, hit
    print("tools/call searchBooks ok:", hit.splitlines()[0])

    count_items = call_streaming({
        "jsonrpc": "2.0", "id": 6, "method": "tools/call",
        "params": {"name": "countTo", "arguments": {"n": 4}}
    })
    progress = [item["params"]["progress"] for item in count_items if item.get("method") == "notifications/progress"]
    assert progress == [1, 2, 3, 4], count_items
    final = count_items[-1]
    assert final["id"] == 6 and final["result"]["countedTo"] == 4, final
    print("tools/call countTo ok:", progress, "->", final["result"])

    print("ALL CHECKS PASSED")
finally:
    p.terminate()
    try:
        p.wait(timeout=2)
    except subprocess.TimeoutExpired:
        p.kill()
