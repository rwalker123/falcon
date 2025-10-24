const TURN_SESSION_KEY = "demo.logger.last_turn";
const COMMAND_TOPIC = "commands.issue.result";

host.log("info", "demo.logger script booting");

const capabilities = new Set(host.capabilities());

host.register({
  onEvent: "handleEvent",
  subscriptions: ["world.snapshot"],
});

function handleEvent(topic, payload) {
  if (topic === "world.snapshot") {
    const turn = extractTurn(payload);
    if (turn === null) {
      host.log("warn", "snapshot payload missing turn field");
      return;
    }

    const lastTurn = host.sessionGet
      ? host.sessionGet(TURN_SESSION_KEY)
      : null;

    if (lastTurn !== turn) {
      host.log("info", `world.snapshot turn=${turn}`);
      if (host.sessionSet) {
        host.sessionSet(TURN_SESSION_KEY, turn);
      }

      maybeIssueCommand(turn);
      maybeEmitAlert(turn);
    }
  } else if (topic === COMMAND_TOPIC) {
    const ok = payload && payload.ok;
    host.log(
      ok ? "info" : "error",
      `command acknowledgement received (ok=${ok})`
    );
  }
}

function extractTurn(payload) {
  if (!payload || typeof payload !== "object") {
    return null;
  }
  if ("turn" in payload && typeof payload.turn === "number") {
    return payload.turn;
  }
  return null;
}

function maybeIssueCommand(turn) {
  if (!capabilities.has("commands.issue")) {
    return;
  }
  if (turn % 5 !== 0) {
    return;
  }
  host.log("debug", "requesting noop command for demo purposes");
  host.request("commands.issue", {
    line: "noop // issued by demo.logger",
  });
}

function maybeEmitAlert(turn) {
  if (!capabilities.has("alerts.emit")) {
    return;
  }
  if (turn % 10 !== 0) {
    return;
  }
  host.request("alerts.emit", {
    level: "info",
    title: "Demo Logger",
    message: `Processed turn ${turn}`,
  });
}
