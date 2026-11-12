// A small calculator for the launcher's "=" search: numbers, + - * / % ^, brackets and decimals.
// It runs here, in the launcher, so nothing typed leaves the computer (KDE's calculator runner can
// download currency rates, so it is not used). Returns a number, or null when the text is not a sum.
.pragma library

function evaluate(text) {
    const s = String(text).replace(/×/g, "*").replace(/÷/g, "/").replace(/,/g, ".").replace(/\s+/g, "")
    if (s.length === 0 || s.length > 200 || !/^[0-9+\-*/%^().]+$/.test(s)) {
        return null
    }
    let i = 0
    function peek() { return s[i] }
    function number() {
        const start = i
        while (i < s.length && /[0-9.]/.test(s[i])) i++
        const t = s.slice(start, i)
        if (!/^(\d+\.?\d*|\.\d+)$/.test(t)) throw "bad number"
        return parseFloat(t)
    }
    function factor() {
        if (peek() === "-") { i++; return -factor() }
        if (peek() === "+") { i++; return factor() }
        let v
        if (peek() === "(") {
            i++
            v = sum()
            if (peek() !== ")") throw "missing )"
            i++
        } else {
            v = number()
        }
        if (peek() === "^") { i++; v = Math.pow(v, factor()) }
        return v
    }
    function product() {
        let v = factor()
        while (peek() === "*" || peek() === "/" || peek() === "%") {
            const op = s[i++]
            const r = factor()
            v = op === "*" ? v * r : op === "/" ? v / r : v % r
        }
        return v
    }
    function sum() {
        let v = product()
        while (peek() === "+" || peek() === "-") {
            const op = s[i++]
            const r = product()
            v = op === "+" ? v + r : v - r
        }
        return v
    }
    try {
        const v = sum()
        if (i !== s.length || !isFinite(v)) return null
        return v
    } catch (e) {
        return null
    }
}

// The answer as people write it: no float noise (0.1+0.2 shows 0.3), at most 10 decimals.
function format(v) {
    return String(parseFloat(v.toPrecision(12)))
}
