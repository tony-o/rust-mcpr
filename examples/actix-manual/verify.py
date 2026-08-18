import json
import subprocess
import sys
import time
import urllib.request

binary = sys.argv[1]
url = "http://127.0.0.1:3001/mcp"

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
    # SSE body: read raw "data: <json>\n\n" frames off the socket as they arrive, rather
    # than buffering the whole response, since the point is that frames show up incrementally.
    req = urllib.request.Request(
        url,
        data=json.dumps(obj).encode(),
        headers={"content-type": "application/json"},
        method="POST",
    )
    items = []
    with urllib.request.urlopen(req, timeout=5) as resp:
        while True:
            line = resp.readline()
            if not line:
                break
            line = line.decode().strip()
            if line.startswith("data: "):
                items.append(json.loads(line[len("data: ") :]))
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

    # streaming tool: progress notifications over SSE, then the final id-wrapped result
    frames = call_streaming({
        "jsonrpc": "2.0", "id": 6, "method": "tools/call",
        "params": {"name": "countTo", "arguments": {"n": 4}}
    })

    *progress_frames, final = frames
    progress_seen = [f["params"]["progress"] for f in progress_frames]
    assert progress_seen == [1, 2, 3, 4], frames
    print("countTo progress notifications ok:", progress_seen)

    assert final["id"] == 6, final
    assert final["result"] == {"countedTo": 4}, final
    print("countTo final result ok:", final["result"])

    print("ALL CHECKS PASSED")
finally:
    p.terminate()
    try:
        p.wait(timeout=2)
    except subprocess.TimeoutExpired:
        p.kill()
