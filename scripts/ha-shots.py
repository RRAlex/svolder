#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Helper of `make ha-shots` (scripts/ha-shots.sh): what the script asks of Home Assistant and of
headless Chromium.

  ha-shots.py absent          fail when Home Assistant already lists the rig's ids as live
  ha-shots.py appear          wait until the rig's entities are live in Home Assistant
  ha-shots.py wait ROLE V     wait until the rig's volume or crossfeed has the state V (a number:
                              within 0.5)
  ha-shots.py shoot OUTDIR    draw the dashboard page into OUTDIR/desktop.png and phone.png
  ha-shots.py selftest        run the entity matcher against a fake registry (no Home Assistant)
  ha-shots.py after           list what Home Assistant still holds under the rig's names

Environment (the script checks them first): HA_URL, HA_TOKEN_FILE, HA_SHOTS_DASHBOARD (the url path
of the dashboard that runs the custom:svolder strategy), HA_SHOTS_PROFILE (a Chromium profile
directory the script removes), HA_SHOTS_CHECKOUT (the checkout whose contrib/home-assistant files
the page runs). The rig is found by its ids, never by a name: its devices by the MQTT identifiers
svolder_<unit> and svolder_computer_desk, its entities by an mqtt unique_id that starts with
those, so a light called Desk or a real device called Bifrost 3 is never read as the rig's. The
token is read from its file and never printed or put on a command line.

Home Assistant is only read: REST GETs and the registry lists over its websocket. Nothing here
calls a service, writes a registry entry or edits a dashboard.
"""

import asyncio
import base64
import fcntl
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

import websockets

# The rig: the fake computer and the fake DAC, and the ids Home Assistant keeps them under (the
# unit is the one scripts/ha-shots.sh gives the fake device; the computer's id is its host name).
COMPUTER = "Desk"
UNIT = "0A1B2C3D4E5F6071"
DEVICE_IDS = ("svolder_" + UNIT.lower(), "svolder_computer_desk")
ROLES = {"volume": "svolder_%s_volume" % UNIT.lower(),
         "crossfeed": "svolder_computer_desk_computer_crossfeed"}
DASHBOARD_THEME = "svolder"

# The desktop picture's page is 1240 css px wide with Home Assistant's 256 px sidebar, so the
# dashboard area is 984 css px; both pictures are drawn at twice that.
# `skip` names the sections the picture ends above (the diagnostics list); `cut`, when set, is a
# card tag the picture ends under instead: the phone's single column stops after the computer's
# chain card, leaving out the "other" entities list, with a margin small enough to leave the next
# heading out; `margin` is the css px kept under the last card.
DESKTOP = {"width": 1240, "height": 3200, "scale": 2, "mobile": False,
           "skip": "diagnostics", "cut": "",
           "margin": 16}
PHONE = {"width": 412, "height": 3200, "scale": 2, "mobile": True,
         "skip": "diagnostics", "cut": "svolder-chain-card",
         "margin": 4}

URL = os.environ.get("HA_URL", "").rstrip("/")


def token():
    with open(os.environ["HA_TOKEN_FILE"], encoding="utf-8") as f:
        return f.read().strip()


def get(path):
    """GET a Home Assistant REST path: (status, parsed body or None)."""
    req = urllib.request.Request(URL + path)
    req.add_header("Authorization", "Bearer " + token())
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        return e.code, None
    except OSError:
        return 0, None


def state(entity):
    code, body = get("/api/states/" + entity)
    return body["state"] if code == 200 and body else None


def live(s):
    return s not in ("unavailable", "unknown")


async def registry():
    """The device and entity registries, read over the websocket."""
    ws_url = URL.replace("http", "ws", 1) + "/api/websocket"
    async with websockets.connect(ws_url, max_size=None) as ws:
        await ws.recv()
        await ws.send(json.dumps({"type": "auth", "access_token": token()}))
        if json.loads(await ws.recv()).get("type") != "auth_ok":
            sys.exit("FAIL: Home Assistant refused the token")
        out = {}
        for i, kind in enumerate(("device", "entity"), 1):
            await ws.send(json.dumps({"id": i, "type": "config/%s_registry/list" % kind}))
            while True:
                r = json.loads(await ws.recv())
                if r.get("id") == i:
                    out[kind] = r["result"]
                    break
        return out


def mqtt_ids(device):
    return [str(p[1]).lower() for p in device.get("identifiers", [])
            if isinstance(p, (list, tuple)) and len(p) == 2 and p[0] == "mqtt"]


def rig_devices(reg):
    """The registry devices with exactly one of the rig's MQTT identifiers."""
    return [d for d in reg["device"] if set(mqtt_ids(d)) & set(DEVICE_IDS)]


def rig_entities(reg):
    """The rig's entities: {unique_id: entity_id} of the mqtt entities that belong to one of the
    rig's devices, or whose unique id is the fake unit's id plus `_` and a key. A computer entity is
    matched through its device, never by a prefix of its unique id: a real computer whose host name
    starts with `desk` has its own device and unique ids that share that prefix."""
    devices = {d["id"] for d in rig_devices(reg)}
    own = "svolder_%s_" % UNIT.lower()
    return {str(e["unique_id"]).lower(): e["entity_id"] for e in reg["entity"]
            if e.get("platform") == "mqtt" and e.get("unique_id")
            and (e.get("device_id") in devices or str(e["unique_id"]).lower().startswith(own))}


def entity_roles(reg):
    """The entity id of each role (volume, crossfeed) that the rig has, by exact unique id."""
    entities = rig_entities(reg)
    return {role: entities[uid] for role, uid in ROLES.items() if uid in entities}


def entity_of(role):
    return entity_roles(asyncio.run(registry())).get(role)


def wait_for(role, ok, what, seconds=90):
    end = time.monotonic() + seconds
    last = None
    entity = None
    while time.monotonic() < end:
        entity = entity or entity_of(role)
        last = state(entity) if entity else None
        if last is not None and ok(last):
            print("ok: the rig's %s %s (%s)" % (role, what, last))
            return
        time.sleep(1)
    sys.exit("FAIL: the rig's %s did not %s within %ds (last: %s)" % (role, what, seconds, last))


def cmd_absent():
    code, _ = get("/api/")
    if code != 200:
        sys.exit("FAIL: Home Assistant's REST API answered %s" % code)
    reg = asyncio.run(registry())
    mine = rig_entities(reg)
    busy = sorted(e for e in mine.values() if live(state(e)))
    if busy:
        sys.exit("FAIL: Home Assistant already lists live entities under the rig's ids (%s): "
                 "another daemon publishes the same unit or a computer named desk; "
                 "refusing to share their unique ids" % ", ".join(busy[:5]))
    print("ok: Home Assistant answers and none of the rig's entities is live (%d stale)"
          % len(mine))


def cmd_appear():
    wait_for("volume", live, "appeared and is available")
    wait_for("crossfeed", live, "appeared and is available")


def cmd_wait(role, want):
    if role not in ROLES:
        sys.exit("unknown role %s" % role)
    try:
        goal = float(want)
        ok = lambda s: live(s) and abs(float(s) - goal) < 0.5  # noqa: E731
    except ValueError:
        ok = lambda s: s == want  # noqa: E731
    wait_for(role, ok, "is %s" % want)


def cmd_after():
    reg = asyncio.run(registry())
    entities = rig_entities(reg)
    devices = [d.get("name_by_user") or d.get("name") for d in rig_devices(reg)]
    states = ["%s=%s" % (e, state(e)) for e in sorted(entities.values()) if state(e) is not None]
    print("rig states still in HA: %s" % (", ".join(states) or "none"))
    print("rig devices still in the device registry: %s" % (", ".join(map(str, devices)) or "none"))
    names = ", ".join(sorted(entities.values())) or "none"
    print("rig entities still in the entity registry: %s" % names)
    if states or devices or entities:
        sys.exit(1)


def view_path(reg):
    """The path the strategy gives the rig's computer: views are the computers in (name, id) order,
    each path the name's slug with an underscore added for every earlier view of the same slug."""
    comps = []
    for d in reg["device"]:
        if d.get("disabled_by"):
            continue
        ids = mqtt_ids(d)
        cid = next((i for i in ids if re.match(r"^svolder_computer(_[a-z0-9_]+)?$", i)), None)
        if cid:
            comps.append(((d.get("name_by_user") or d.get("name") or "This computer"), cid))
    comps.sort(key=lambda c: (c[0].casefold(), c[1]))
    seen = set()
    for name, cid in comps:
        path = re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_") or "computer"
        while path in seen:
            path += "_"
        seen.add(path)
        if cid == "svolder_computer_desk":
            return path
    sys.exit("FAIL: the rig's computer is not in Home Assistant's device registry")


# ---- Chromium ---------------------------------------------------------------------------------

# Where Home Assistant loads the cards from: a pinned jsDelivr URL of the public repository, or the
# /local/ copy a person made by hand. Either way the file under contrib/home-assistant/ of this
# checkout is served in its place.
RESOURCE = re.compile(r"^(?:https://[^/]+/.*?/contrib/home-assistant/|[^?#]*?/local/)"
                      r"(svolder\.js|svolder/[A-Za-z0-9_./-]+\.js)(?:[?#].*)?$")

# Everything inside the page that must hold before it is shot: the strategy drew the view, and no
# card is an error card.
PAGE_STATE = r"""(() => {
  const SKIP = new RegExp(%SKIP%, 'i');
  const CUT = %CUT%;
  const MARGIN = %MARGIN%;
  const cut = [];
  const found = {}; const errs = []; let view = null; const faces = [];
  const walk = (n) => {
    if (!n) return;
    if (n.nodeType === 1) {
      const t = n.tagName.toLowerCase();
      if (t.startsWith('svolder-')) found[t] = (found[t] || 0) + 1;
      if (t === 'hui-error-card' || t === 'hui-warning') errs.push(t);
      if (t === 'hui-view' && !view) view = n;
      if (CUT && t === CUT) cut.push(n);
      if (n.shadowRoot) walk(n.shadowRoot);
    }
    for (const c of n.childNodes) walk(c);
  };
  walk(document);
  // The picture ends under the last section that SKIP does not name.
  const text = (n) => {
    let t = n.nodeType === 3 ? n.textContent : '';
    if (n.shadowRoot) t += text(n.shadowRoot);
    for (const c of n.childNodes) t += text(c);
    return t;
  };
  const sections = [];
  const gather = (n) => {
    if (!n) return;
    if (n.nodeType === 1 && n.tagName.toLowerCase() === 'hui-section') sections.push(n);
    if (n.shadowRoot) gather(n.shadowRoot);
    for (const c of n.childNodes) gather(c);
  };
  if (view) gather(view);
  let rect = null;
  if (view) {
    const r = view.getBoundingClientRect();
    let bottom = 0;
    for (const sec of sections) {
      if (SKIP.test(text(sec))) continue;
      bottom = Math.max(bottom, sec.getBoundingClientRect().bottom);
    }
    if (cut.length) bottom = cut[cut.length - 1].getBoundingClientRect().bottom;
    rect = [r.x, r.y, r.width, bottom ? Math.ceil(bottom - r.y) + MARGIN : r.height];
  }
  return JSON.stringify({ path: location.pathname, found, errs, rect });
})()"""


# What the cards need from this checkout on every page: the entry, the strategy and the four cards.
REQUIRED = {"svolder.js", "svolder/strategy.js", "svolder/cards/index.js",
            "svolder/cards/faceplate.js",
            "svolder/cards/eq.js", "svolder/cards/scenes.js", "svolder/cards/chain.js"}


class Browser:
    """Headless Chromium driven over a pipe (--remote-debugging-pipe: file descriptors 3 and 4, so
    no TCP port is open for another local process to attach to and read the token from). The
    profile directory is 0700."""

    def __init__(self, profile, checkout):
        self.profile = profile
        self.root = os.path.join(checkout, "contrib", "home-assistant")
        self.seq = 0
        self.served = set()
        self.session = None

    async def __aenter__(self):
        shutil.rmtree(self.profile, ignore_errors=True)
        os.makedirs(self.profile, mode=0o700)
        os.chmod(self.profile, 0o700)
        to_r, self.to_w = os.pipe()
        self.from_r, from_w = os.pipe()
        # High numbers first, so that placing them on 3 and 4 never overwrites one of them.
        low = (to_r, from_w)
        to_r, from_w = (fcntl.fcntl(fd, fcntl.F_DUPFD, 100) for fd in low)
        for fd in low:
            os.close(fd)

        def child():
            os.dup2(to_r, 3)
            os.dup2(from_w, 4)

        self.proc = subprocess.Popen(
            ["chromium", "--headless", "--remote-debugging-pipe", "--user-data-dir=" + self.profile,
             "--no-first-run", "--hide-scrollbars", "--disable-gpu", "--force-color-profile=srgb",
             "about:blank"],
            pass_fds=(3, 4), preexec_fn=child, stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.close(to_r)
        os.close(from_w)
        self.reader = asyncio.StreamReader(limit=1 << 28)
        await asyncio.get_running_loop().connect_read_pipe(
            lambda: asyncio.StreamReaderProtocol(self.reader), os.fdopen(self.from_r, "rb", 0))
        made = await self.call("Target.createTarget", url="about:blank")
        att = await self.call("Target.attachToTarget", targetId=made["targetId"], flatten=True)
        self.session = att["sessionId"]
        return self

    async def __aexit__(self, *_):
        self.proc.terminate()
        try:
            self.proc.wait(10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        os.close(self.to_w)
        shutil.rmtree(self.profile, ignore_errors=True)

    async def send(self, method, **params):
        self.seq += 1
        msg = {"id": self.seq, "method": method, "params": params}
        if self.session:
            msg["sessionId"] = self.session
        os.write(self.to_w, json.dumps(msg).encode() + b"\0")
        return self.seq

    async def recv(self, timeout=None):
        data = await asyncio.wait_for(self.reader.readuntil(b"\0"), timeout)
        return json.loads(data[:-1])

    async def paused(self, ev):
        p = ev["params"]
        m = RESOURCE.match(p["request"]["url"])
        local = os.path.normpath(os.path.join(self.root, m.group(1))) if m else None
        if local and local.startswith(self.root + os.sep) and os.path.isfile(local):
            with open(local, "rb") as f:
                body = base64.b64encode(f.read()).decode()
            self.served.add(m.group(1))
            await self.send("Fetch.fulfillRequest", requestId=p["requestId"], responseCode=200,
                            responseHeaders=[
                                {"name": "Content-Type", "value": "application/javascript"},
                                {"name": "Access-Control-Allow-Origin", "value": "*"}],
                            body=body)
        else:
            await self.send("Fetch.continueRequest", requestId=p["requestId"])

    async def call(self, method, **params):
        want = await self.send(method, **params)
        while True:
            r = await self.recv(60)
            if r.get("method") == "Fetch.requestPaused":
                await self.paused(r)
            elif r.get("id") == want:
                if "error" in r:
                    sys.exit("FAIL: Chromium refused %s: %s" % (method, r["error"]))
                return r.get("result", {})

    async def pump(self, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            try:
                r = await self.recv(0.3)
            except asyncio.TimeoutError:
                continue
            if r.get("method") == "Fetch.requestPaused":
                await self.paused(r)

    async def eval(self, js):
        r = await self.call("Runtime.evaluate", expression=js, returnByValue=True)
        return r.get("result", {}).get("value")


async def shoot(outdir):
    dash = os.environ.get("HA_SHOTS_DASHBOARD", "svolder-auto").strip("/")
    view = "%s/%s/%s" % (URL, dash, view_path(await registry()))
    tok = json.dumps({"access_token": token(), "token_type": "Bearer", "expires_in": 1800,
                      "hassUrl": URL, "clientId": URL + "/", "expires": 4102444800000,
                      "refresh_token": ""})
    os.makedirs(outdir, exist_ok=True)
    async with Browser(os.environ["HA_SHOTS_PROFILE"], os.environ["HA_SHOTS_CHECKOUT"]) as b:
        await b.call("Page.enable")
        await b.call("Network.enable")
        await b.call("Network.setCacheDisabled", cacheDisabled=True)
        await b.call("Fetch.enable", patterns=[{"urlPattern": "*", "requestStage": "Request"}])
        await b.call("Page.navigate", url=URL + "/auth/authorize")
        await b.pump(3)
        await b.eval("localStorage.setItem('hassTokens', %s);"
                     "localStorage.setItem('selectedTheme', JSON.stringify({theme:%s}))"
                     % (json.dumps(tok), json.dumps(DASHBOARD_THEME)))
        for name, size in (("desktop", DESKTOP), ("phone", PHONE)):
            state_js = PAGE_STATE.replace("%SKIP%", json.dumps(size["skip"])).replace(
                "%CUT%", json.dumps(size["cut"])).replace("%MARGIN%", str(size["margin"]))
            await b.call("Emulation.setDeviceMetricsOverride", width=size["width"],
                         height=size["height"], deviceScaleFactor=size["scale"],
                         mobile=size["mobile"])
            b.served.clear()
            await b.call("Page.navigate", url=view)
            got = None
            end = time.monotonic() + 90
            while time.monotonic() < end:
                await b.pump(1.5)
                got = json.loads(await b.eval(state_js))
                if (got["found"].get("svolder-faceplate-card")
                        and got["found"].get("svolder-eq-card")
                        and got["found"].get("svolder-scenes-card") and got["rect"]
                        and got["rect"][3] > 300):
                    break
            else:
                sys.exit("FAIL: the %s page never drew the rig's cards: %s" % (name, got))
            if got["errs"]:
                sys.exit("FAIL: error cards on the %s page: %s" % (name, got["errs"]))
            if not REQUIRED <= b.served:
                missing = ", ".join(sorted(REQUIRED - b.served))
                sys.exit("FAIL: the %s page did not run these card files from the checkout: %s "
                         "(does the dashboard's resource point at svolder.js?)" % (name, missing))
            await b.pump(4)  # the cards finish their first draw
            got = json.loads(await b.eval(state_js))
            x, y, w, h = got["rect"]
            shot = await b.call("Page.captureScreenshot", format="png", captureBeyondViewport=False,
                                clip={"x": x, "y": y, "width": w, "height": h, "scale": 1})
            path = os.path.join(outdir, name + ".png")
            with open(path, "wb") as f:
                f.write(base64.b64decode(shot["data"]))
            print("%s: %s at %dx%d css px, cards %s, %d card files served from the checkout"
                  % (name, path, w, h, sorted(got["found"]), len(b.served)))


def selftest():
    """The matcher against a fake registry: the rig is found by its devices and unit, and a real
    computer called desk-pc, a light called Desk and a real Bifrost 3 are not."""
    def dev(i, *ids):
        return {"id": i, "identifiers": [["mqtt", x] for x in ids], "name": i}

    def ent(eid, uid, device, platform="mqtt"):
        return {"entity_id": eid, "unique_id": uid, "device_id": device, "platform": platform}

    reg = {
        "device": [dev("rigdac", "svolder_" + UNIT), dev("rigpc", "svolder_computer_desk"),
                   dev("realpc", "svolder_computer_desk_pc"),
                   dev("realdac", "svolder_2CF60DC47EE5D4A4"),
                   dev("lamp", "esphome_lamp")],
        "entity": [
            ent("number.bifrost_3_volume_2", "svolder_%s_volume" % UNIT, "rigdac"),
            ent("sensor.bifrost_3_extra", "svolder_%s_extra" % UNIT.lower(), "rigdac"),
            ent("switch.desk_crossfeed", "svolder_computer_desk_computer_crossfeed", "rigpc"),
            ent("switch.desk_pc_crossfeed", "svolder_computer_desk_pc_computer_crossfeed",
                "realpc"),
            ent("number.bifrost_3_volume", "svolder_2CF60DC47EE5D4A4_volume", "realdac"),
            ent("light.desk_lamp", "lamp_1", "lamp", platform="esphome"),
            ent("light.desk_lamp_mqtt", "desk_lamp_mqtt", None),
        ],
    }
    got = set(rig_entities(reg).values())
    want = {"number.bifrost_3_volume_2", "sensor.bifrost_3_extra", "switch.desk_crossfeed"}
    assert got == want, "matched %s, wanted %s" % (sorted(got), sorted(want))
    assert {d["id"] for d in rig_devices(reg)} == {"rigdac", "rigpc"}
    assert entity_roles(reg) == {"volume": "number.bifrost_3_volume_2",
                                 "crossfeed": "switch.desk_crossfeed"}
    print("ok: the matcher finds the rig's 3 entities and 2 devices and nothing else")


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "selftest":
        return selftest()
    if what in ("absent", "appear", "after", "shoot", "wait") and not URL:
        sys.exit("HA_URL is not set")
    if what == "absent":
        cmd_absent()
    elif what == "appear":
        cmd_appear()
    elif what == "wait" and len(sys.argv) == 4:
        cmd_wait(sys.argv[2], sys.argv[3])
    elif what == "after":
        cmd_after()
    elif what == "shoot" and len(sys.argv) == 3:
        asyncio.run(shoot(sys.argv[2]))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
