extends RefCounted
class_name ScenarioRefusals

## All-`static` reading of the server's refusal lines (`write_scenario` / `load_scenario` /
## `read_scenario`). A line is `"<path in the file>: <reason>"` — `factions[0].labor[2].assign_labor.workers:
## band "x" is not declared` — or `"<file>: <reason>"`, or a serde `line L, column C` message. Only the
## first kind names a node in the form; the rest go to the banner.
##
## The path is split on the FIRST `": "` and accepted only if it is made of key and `[index]` tokens, so a
## reason that itself contains a colon never moves the split, and a `line 3, column 9` message (which has
## spaces and a comma before any colon) is never mistaken for a path.

## `key`, then any mix of `[n]` and `.key`. Key characters are what a JSON key in these files is made of.
const PATH_PATTERN := "^[A-Za-z_][A-Za-z0-9_\\-]*(\\[[0-9]+\\])*(\\.[A-Za-z0-9_\\-]+(\\[[0-9]+\\])*)*$"
const PATH_REASON_SEPARATOR := ": "

static var _path_regex: RegEx = null


## `{path, reason, line}` — `path` is `""` when the line names no node.
static func parse(line: String) -> Dictionary:
	var out := {"path": "", "reason": line, "line": line}
	var split := line.find(PATH_REASON_SEPARATOR)
	if split <= 0:
		return out
	var head := line.substr(0, split)
	if _regex().search(head) == null:
		return out
	out["path"] = head
	out["reason"] = line.substr(split + PATH_REASON_SEPARATOR.length())
	return out


static func parse_all(lines: Array) -> Array:
	var out: Array = []
	for line in lines:
		out.append(parse(String(line)))
	return out


## Is `node_path` the refusal path itself or one of its ancestors? `a.b` is an ancestor of `a.b.c` and of
## `a.b[0]`, never of `a.bc`.
static func is_prefix(node_path: String, refusal_path: String) -> bool:
	if node_path.is_empty() or refusal_path.is_empty():
		return false
	if refusal_path == node_path:
		return true
	if not refusal_path.begins_with(node_path):
		return false
	var next := refusal_path[node_path.length()]
	return next == "." or next == "["


static func _regex() -> RegEx:
	if _path_regex == null:
		_path_regex = RegEx.new()
		_path_regex.compile(PATH_PATTERN)
	return _path_regex
