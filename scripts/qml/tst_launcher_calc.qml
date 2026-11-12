import QtQuick
import QtTest
import "../../plasma-launcher/contents/ui/Calc.js" as Calc

// The launcher's "=" calculator. It must work out ordinary sums and refuse anything else.
TestCase {
    name: "LauncherCalc"

    function test_sums_data() {
        return [
            { tag: "precedence", text: "2+3*4", want: 14 },
            { tag: "brackets", text: "12*(3+4)/2", want: 42 },
            { tag: "power", text: "2^10", want: 1024 },
            { tag: "remainder", text: "17%5", want: 2 },
            { tag: "minus first", text: "-3+5", want: 2 },
            { tag: "decimal comma", text: "1,5*2", want: 3 },
            { tag: "times and divide signs", text: "6×7÷2", want: 21 },
            { tag: "spaces", text: " 1234 / 2 ", want: 617 },
        ]
    }
    function test_sums(d) {
        compare(Calc.evaluate(d.text), d.want)
    }

    function test_not_sums_data() {
        return [
            { tag: "empty", text: "" },
            { tag: "letters", text: "firefox" },
            { tag: "trailing operator", text: "12+" },
            { tag: "open bracket", text: "(1+2" },
            { tag: "divide by zero", text: "1/0" },
            { tag: "code", text: "alert(1)" },
            { tag: "two dots", text: "1.2.3" },
        ]
    }
    function test_not_sums(d) {
        compare(Calc.evaluate(d.text), null)
    }

    function test_format() {
        compare(Calc.format(0.1 + 0.2), "0.3")
        compare(Calc.format(10 / 3), "3.33333333333")
    }
}
