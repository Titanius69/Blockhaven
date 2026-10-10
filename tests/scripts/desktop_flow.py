"""Exercise the real dispatcher with a Java stand-in, never a game download."""
import json
from pathlib import Path
import subprocess
import tempfile
import time

app = Path(__file__).resolve().parents[2]
binary = app / "target/debug/blockhaven"
with tempfile.TemporaryDirectory() as temporary:
    root = Path(temporary)
    capture = root / "arguments.json"
    java = root / "java"
    java.write_text("#!/usr/bin/env python3\nimport sys,json,time\nfrom pathlib import Path\nif '-version' in sys.argv:\n print('openjdk version \\\"21.0.1\\\"',file=sys.stderr)\nelse:\n Path(" + repr(str(capture)) + ").write_text(json.dumps(sys.argv[1:]))\n print('Minecraft stand-in running',flush=True)\n time.sleep(20)\n")
    java.chmod(0o755)
    process = subprocess.Popen([str(binary), "--test-root", str(root), "--stdio"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    def action(action_name, value="", **fields):
        process.stdin.write(json.dumps({"action": action_name, "value": value, "fields": {k: [str(v)] for k, v in fields.items()}}) + "\n")
        process.stdin.flush()
        return json.loads(process.stdout.readline())
    def wait_until(predicate):
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline:
            result = action("poll")
            if predicate(result):
                return result
            time.sleep(0.05)
        raise AssertionError("Timed out waiting for launcher update")
    try:
        action("add_offline", name="Tester")
        action("create_profile", name="Test instance", version="1.21.1", loader="vanilla", memory=4096)
        wait_until(lambda result: (root / "config.json").exists() and len(json.loads((root / "config.json").read_text())["profiles"]) == 1)
        config = json.loads((root / "config.json").read_text())
        instance = config["profiles"][0]
        instance_id = instance["id"]
        action("save_settings", java=java, game_root=root / "games", memory=4096, width=1280, height=720, workers=4, client_id="", discord_enabled="on", discord_application_id="123456789012345678", discord_image_key="")
        saved = json.loads((root / "config.json").read_text())
        assert saved["settings"]["discord"]["application_id"] == "123456789012345678"
        action("open_profile", value=instance_id)
        action("open_version", value=instance_id)
        assert (root / "games/instances" / instance_id).is_dir()
        version = root / "games/versions/1.21.1"
        assert version.is_dir()
        receipt = {"minecraft": "1.21.1", "loader": "vanilla", "loader_version": "", "java_major": 21, "main_class": "Test", "metadata": json.loads((app / "tests/fixtures/minecraft-1.21.1.json").read_text()), "classpath": [], "native_jars": [], "platform": {"os": "linux", "arch": "x86_64", "version": ""}, "asset_index": "17"}
        (version / "installed-linux-x86_64.json").write_text(json.dumps(receipt))
        action("play")
        hidden = wait_until(lambda result: result.get("window", {}).get("visible") is False)
        assert hidden["window"]["focus"] is False
        deadline = time.monotonic() + 3
        while not capture.exists() and time.monotonic() < deadline:
            time.sleep(0.02)
        arguments = json.loads(capture.read_text())
        assert "Tester" in arguments
        assert saved["offline_accounts"][0]["uuid"].replace("-", "") in arguments or saved["offline_accounts"][0]["uuid"] in arguments
        action("stop")
        restored = wait_until(lambda result: result.get("window", {}).get("visible") is True)
        assert restored["window"]["focus"] is True
        assert any("Minecraft exited" in str(p) for p in restored["patches"])
        assert action("poll").get("window") is None
        # A failed Java launch must never hide the launcher.
        action("save_settings", java=root / "missing-java", game_root=root / "games", memory=4096, width=1280, height=720, workers=4, client_id="", discord_application_id="", discord_image_key="")
        action("play")
        failure = wait_until(lambda result: "Java could not be started" in json.dumps(result))
        assert failure.get("window") is None
        print("PASS: offline launch, folder actions, Discord settings, hide, stop/restore and failed launch")
    finally:
        process.stdin.close()
        process.wait(timeout=5)
