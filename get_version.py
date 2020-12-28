import json
from subprocess import check_output

manifest = json.loads(check_output(["cargo", "read-manifest", "--manifest-path", ".\\botw-freecam\\Cargo.toml"]))

print("v" + manifest['version'])