#!/usr/bin/env bash
# sync.sh — Client-side incremental repository sync for chum-memory.
# Called by the plugin hook on every user prompt. Detects changed files,
# sends only diffs plus a full manifest to the API, then reconciles the
# local manifest using acceptedPaths/missingPaths from the response.
#
# Usage: sync.sh [ROOT_DIR] [API_URL]

set -uo pipefail

ROOT_DIR="${1:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
API_URL="${2:-${CHUM_MEMORY_API_URL:-http://localhost:63001}}"
CACHE_DIR="${ROOT_DIR}/.chum-cache"
RULES_FILE="${CACHE_DIR}/sync-rules.json"
PROJECT_ID="${CHUM_MEM_PROJECT_ID:-}"

mkdir -p "$CACHE_DIR"

if [[ ! -f "$RULES_FILE" ]]; then
  curl -sf --max-time 5 "${API_URL}/api/knowledge/sync-rules" > "$RULES_FILE" 2>/dev/null || {
    cat > "$RULES_FILE" <<'RULES'
{"codeExtensions":["ts","tsx","js","jsx","mjs","cjs","py","go","rs","java","c","cc","cpp","h","hpp","rb","cs","kt","scala","php","swift","lua","zig","ps1","sh","sql","html","htm","css","scss","sass","less","vue","svelte","astro"],"docExtensions":["md","mdx","txt","rst","yaml","yml","json","jsonc"],"ignoreDirs":[".git","node_modules","dist","build","out","target","__pycache__","venv",".venv",".next",".nuxt","coverage",".turbo",".cache","graphify-out"],"ignoreFiles":[".DS_Store","package-lock.json","pnpm-lock.yaml","yarn.lock","bun.lockb","Cargo.lock"],"ignorePatterns":[".env*","*.pem","*.key","*.crt","*.min.js","*.min.css","*.map","*.d.ts","*.generated.ts","*.generated.js"],"maxFileSizeBytes":262144}
RULES
  }
fi

cd "$ROOT_DIR"

PAYLOAD_FILE=$(mktemp /tmp/chum-sync-payload.XXXXXX)
STATE_FILE=$(mktemp /tmp/chum-sync-state.XXXXXX)
trap 'rm -f "$PAYLOAD_FILE" "$STATE_FILE"' EXIT

RESULT=$(python3 -s - "$CACHE_DIR" "$RULES_FILE" "$PROJECT_ID" "$PAYLOAD_FILE" "$STATE_FILE" <<'PYTHON'
import hashlib, json, os, subprocess, sys, fnmatch

cache_dir, rules_file, project_id, payload_file, state_file = sys.argv[1:6]
manifest_file = os.path.join(cache_dir, "manifest.tsv")
rejected_file = os.path.join(cache_dir, "rejected.tsv")

with open(rules_file) as f:
    rules = json.load(f)

valid_exts = set(rules.get("codeExtensions", []) + rules.get("docExtensions", []))
ignore_files = set(rules.get("ignoreFiles", []))
ignore_patterns = rules.get("ignorePatterns", [])
max_size = rules.get("maxFileSizeBytes", 262144)

try:
    out = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        stderr=subprocess.DEVNULL, text=True
    )
except subprocess.CalledProcessError:
    out = ""

eligible = []
for filepath in out.strip().splitlines():
    if not filepath:
        continue
    basename = os.path.basename(filepath)
    ext = filepath.rsplit(".", 1)[-1].lower() if "." in filepath else ""
    if ext not in valid_exts or basename in ignore_files:
        continue
    if any(fnmatch.fnmatch(basename, pat) for pat in ignore_patterns):
        continue
    if not os.path.isfile(filepath):
        continue
    try:
        if os.path.getsize(filepath) > max_size:
            continue
    except OSError:
        continue
    eligible.append(filepath)

current = {}
for filepath in eligible:
    try:
        current[filepath] = hashlib.sha256(open(filepath, "rb").read()).hexdigest()
    except Exception:
        pass

def load_tsv(path):
    result = {}
    if os.path.isfile(path):
        with open(path) as f:
            for line in f:
                parts = line.rstrip("\n").split("\t", 1)
                if len(parts) == 2:
                    result[parts[0]] = parts[1]
    return result

old_manifest = load_tsv(manifest_file)
rejected = load_tsv(rejected_file)

# Send files whose content differs from server-confirmed state AND that we
# haven't already been rejected for at this exact hash.
to_send = [
    p for p, h in current.items()
    if old_manifest.get(p) != h and rejected.get(p) != h
]
removed = [p for p in old_manifest if p not in current]

if not to_send and not removed:
    print(json.dumps({"status": "NO_CHANGES", "filesAdded": 0, "filesRemoved": 0, "filesUnchanged": len(current)}))
    sys.exit(0)

files_payload = []
for p in to_send:
    try:
        content = open(p, "r", errors="replace").read()
    except Exception:
        continue
    files_payload.append({"path": p, "hash": current[p], "content": content})

payload = {
    "files": files_payload,
    "removedPaths": removed,
    "manifest": current,
    "mergeWithExisting": True,
}
if project_id:
    payload["projectId"] = project_id

with open(payload_file, "w") as f:
    json.dump(payload, f)

with open(state_file, "w") as f:
    json.dump({
        "current": current,
        "old_manifest": old_manifest,
        "rejected": rejected,
        "sent_paths": [fp["path"] for fp in files_payload],
        "removed_paths": removed,
    }, f)

print(f"SYNC:{len(files_payload)}:{len(removed)}:{len(current)}")
PYTHON
)
PHASE1_RC=$?

if [[ $PHASE1_RC -ne 0 ]]; then
  echo '{"status":"ERROR","error":"Sync script failed in phase 1"}' >&2
  exit 1
fi

if echo "$RESULT" | grep -q '"NO_CHANGES"'; then
  echo "$RESULT"
  exit 0
fi

RESPONSE=$(curl -sf --max-time 120 \
  -X POST \
  -H "Content-Type: application/json" \
  -d @"$PAYLOAD_FILE" \
  "${API_URL}/api/knowledge/repository-sync" 2>/dev/null) || {
  echo "{\"status\":\"ERROR\",\"error\":\"Failed to reach API at ${API_URL}\"}" >&2
  exit 1
}

# Phase 2: reconcile local manifest + rejected tables with server response.
python3 -s - "$CACHE_DIR" "$STATE_FILE" "$RESPONSE" <<'PYTHON'
import json, os, sys

cache_dir, state_file, response_json = sys.argv[1:4]
manifest_file = os.path.join(cache_dir, "manifest.tsv")
rejected_file = os.path.join(cache_dir, "rejected.tsv")

try:
    response = json.loads(response_json)
except Exception:
    response = {}

if response.get("status") != "SUCCESSFUL":
    sys.exit(0)

with open(state_file) as f:
    state = json.load(f)

current = state["current"]
old_manifest = state["old_manifest"]
rejected = state["rejected"]
sent_paths = set(state["sent_paths"])
removed_paths = set(state["removed_paths"])

# Old servers don't return acceptedPaths/missingPaths. Fall back to
# "everything we sent was accepted" so the manifest still progresses.
if "acceptedPaths" in response:
    accepted = set(response.get("acceptedPaths") or [])
    missing = set(response.get("missingPaths") or [])
else:
    accepted = set(sent_paths)
    missing = set()

# Start from old manifest, drop removed/missing, promote accepted to the hash
# we just sent.
new_manifest = {p: h for p, h in old_manifest.items() if p not in removed_paths and p not in missing}
for p in accepted:
    if p in current:
        new_manifest[p] = current[p]

# Rejected ledger: files server reported missing after we sent them get
# recorded at the hash we tried. Stale entries (hash changed, file gone,
# or now accepted) get pruned.
new_rejected = {}
for p, h in rejected.items():
    if current.get(p) == h and p not in accepted:
        new_rejected[p] = h
for p in missing:
    if p in sent_paths and p in current:
        new_rejected[p] = current[p]

def write_tsv(path, mapping):
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        for k, v in sorted(mapping.items()):
            f.write(f"{k}\t{v}\n")
    os.replace(tmp, path)

write_tsv(manifest_file, new_manifest)
write_tsv(rejected_file, new_rejected)
PYTHON

echo "$RESPONSE"
