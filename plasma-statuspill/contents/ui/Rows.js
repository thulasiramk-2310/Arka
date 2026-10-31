.pragma library

// The six checks arkad reports. `kind` says how its value turns into a state.
var ROWS = [
    { id: "firewall",  title: "Firewall",            prop: "Firewall",       kind: "bool" },
    { id: "dns",       title: "Encrypted DNS (DoT)", prop: "DnsStatus",      kind: "dns" },
    { id: "mac",       title: "MAC randomization",   prop: "MacRandomization", kind: "bool" },
    { id: "ipv6",      title: "IPv6 privacy",        prop: "Ipv6Privacy",    kind: "bool" },
    { id: "hostname",  title: "Hostname privacy",    prop: "HostnamePrivacy", kind: "bool" },
    { id: "sandbox",   title: "Browser sandbox",     prop: "BrowserSandbox", kind: "sandbox" }
]
var STALE_SECS = 90   // arkad checks about every 60 s; the tray expires a check after three 30 s polls

var CONFIGURED = "Configured", ATTENTION = "Needs attention", NOT_SET = "Not configured", UNAVAILABLE = "Unavailable"

// Turn one reported value into a state. Anything unclear is Unavailable, never Configured.
function stateOf(kind, value) {
    if (value === undefined || value === null) return UNAVAILABLE
    if (kind === "bool") return value === true ? CONFIGURED : NOT_SET
    if (kind === "dns") {
        if (value === "Encrypted") return CONFIGURED
        if (value === "Plaintext" || value === "Degraded" || value === "ForcedPlaintext") return ATTENTION
        return UNAVAILABLE
    }
    if (kind === "sandbox") {
        if (value === "Persistent" || value === "Ephemeral" || value === "PrivateWorkspace") return CONFIGURED
        if (value === "None") return NOT_SET
        return UNAVAILABLE
    }
    return UNAVAILABLE
}

// busctl --json=short prints one {"type":..,"data":..} document per property, in the order asked.
function parse(output, props) {
    var lines = String(output).split("\n").filter(function (l) { return l.trim().length > 0 })
    if (lines.length < props.length) return null
    var out = {}
    for (var i = 0; i < props.length; i++) {
        try { out[props[i]] = JSON.parse(lines[i]).data } catch (e) { return null }
    }
    return out
}

function ago(secs) {
    secs = Math.round(secs)
    if (secs < 5) return "just now"
    if (secs < 90) return secs + " seconds ago"
    var m = Math.round(secs / 60)
    if (m < 90) return m + (m === 1 ? " minute ago" : " minutes ago")
    var h = Math.round(m / 60)
    return h + (h === 1 ? " hour ago" : " hours ago")
}
