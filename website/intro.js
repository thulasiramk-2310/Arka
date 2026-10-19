/* The ArkaOS film (43 s). One file, nothing from other sites. It plays only when
   the visitor asks for it (any element with data-film), and has a Skip button. */
(function () {
  'use strict';
  var DUR = 43, W = 1920, H = 1080;
  var qs = new URLSearchParams(location.search), mode = qs.get('intro');

  /* ---------- helpers ---------- */
  var clamp = function (x, a, b) { return Math.min(b === undefined ? 1 : b, Math.max(a === undefined ? 0 : a, x)); };
  var seg = function (t, a, b) { return clamp((t - a) / (b - a)); };
  var eo = function (x) { return 1 - Math.pow(1 - x, 4); };
  var eio = function (x) { return x < .5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2; };
  var back = function (x) { var c1 = 1.5, c3 = c1 + 1; return 1 + c3 * Math.pow(x - 1, 3) + c1 * Math.pow(x - 1, 2); };
  var lerp = function (a, b, x) { return a + (b - a) * x; };
  var esc = function (s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;'); };

  /* ---------- styles ---------- */
  var css = '\
#ai{position:fixed;inset:0;z-index:99999;background:#f4f5f7;overflow:hidden;font-family:var(--sans);color:#111317;-webkit-font-smoothing:antialiased;--ink:#111317;--ink2:#4a505b;--line:#dfe3e8;--ac:#16c784;--acd:#0b7a53}\
#ai *{box-sizing:border-box}\
#ai .s{position:absolute;left:0;top:0;width:1920px;height:1080px;transform-origin:0 0}\
#ai .bg{position:absolute;inset:-200px;background:radial-gradient(40% 40% at 25% 20%,#fff 0,rgba(255,255,255,0) 70%),radial-gradient(35% 35% at 80% 85%,rgba(22,199,132,.07) 0,rgba(22,199,132,0) 70%)}\
#ai .a{position:absolute;white-space:nowrap}\
#ai .c{left:0;width:1920px;text-align:center}\
#ai .serif{font-family:var(--serif)}\
#ai .big{font-size:120px;line-height:1.1;letter-spacing:-.02em;font-weight:300}\
#ai .big b{font-weight:600}\
#ai .acc{color:var(--acd)}\
#ai .cr{display:inline-block;width:.06em;height:.9em;background:var(--ac);margin-left:.08em;vertical-align:-.08em}\
#ai .win0{width:560px;height:350px;background:#fff;border:1px solid var(--line);border-radius:16px;box-shadow:0 24px 50px rgba(17,19,23,.10);overflow:hidden}\
#ai .win0 i{display:block;height:14px;margin:0 28px 14px;background:#e9ecf0;border-radius:7px}\
#ai .win0 .tb{display:flex;gap:8px;align-items:center;padding:16px 20px;border-bottom:1px solid var(--line);font-size:22px;color:var(--ink2);margin-bottom:26px}\
#ai .win0 .tb u{width:12px;height:12px;border-radius:50%;background:#d8dce2;display:block}\
#ai .chip{height:76px;padding:0 34px;display:flex;align-items:center;gap:14px;background:#fff;border:1px solid var(--line);border-radius:999px;font-size:34px;box-shadow:0 14px 34px rgba(17,19,23,.08)}\
#ai .chip u{width:12px;height:12px;border-radius:50%;background:var(--ac)}\
#ai .win{left:760px;top:130px;width:1100px;height:820px;background:#fff;border:1px solid var(--line);border-radius:22px;box-shadow:0 40px 80px rgba(17,19,23,.14);overflow:hidden;transform-origin:50% 50%}\
#ai .wbar{height:76px;display:flex;align-items:center;gap:14px;padding:0 30px;border-bottom:1px solid var(--line);font-size:24px;font-weight:600}\
#ai .wbar span{margin-left:auto;font-weight:400;font-size:21px;color:var(--ink2)}\
#ai .wbody{display:flex;height:744px}\
#ai .side{overflow:hidden;border-right:1px solid var(--line);background:#fafbfc;flex:none}\
#ai .side div{margin:12px;height:48px;line-height:48px;padding:0 14px;border-radius:12px;font-size:20px;color:var(--ink2);white-space:nowrap}\
#ai .side .on{background:#eceff2;color:var(--ink)}\
#ai .main{padding:30px 44px 0;flex:1;min-width:0}\
#ai .main h3{font:500 54px/1.1 var(--serif);margin:0 0 12px;letter-spacing:-.01em}\
#ai .sub{font-size:25px;color:var(--ink2);margin:0 0 22px;white-space:normal;line-height:1.4}\
#ai .row{display:flex;align-items:flex-start;gap:16px;padding:9px 0;border-top:1px solid #edf0f3}\
#ai .row:first-child{border-top:0}\
#ai .row s{width:14px;height:14px;border-radius:50%;background:#cfd5dc;margin-top:12px;flex:none;display:block}\
#ai .row p{margin:0;font-size:25px;font-weight:600;line-height:1.3}\
#ai .row em{display:block;font-style:normal;font-weight:400;font-size:20px;color:var(--ink2);line-height:1.3;white-space:normal}\
#ai .row .st{margin-left:auto;font-size:21px;font-weight:600;color:#9aa2ad;padding-top:2px}\
#ai .note{font-size:21px;color:var(--ink2);margin:14px 0 0;white-space:normal}\
#ai .card{left:860px;width:940px;height:215px;background:#fff;border:1px solid var(--line);border-radius:22px;box-shadow:0 30px 60px rgba(17,19,23,.10);padding:30px 40px}\
#ai .card small{display:block;font-size:19px;letter-spacing:.12em;text-transform:uppercase;color:var(--ink2);margin-bottom:8px}\
#ai .card h4{margin:0 0 6px;font:500 46px/1.15 var(--serif)}\
#ai .card span{font-size:24px;color:var(--ink2)}\
#ai .bar{position:absolute;left:40px;right:40px;bottom:26px;height:8px;background:#edf0f3;border-radius:4px;overflow:hidden}\
#ai .bar i{display:block;height:100%;width:0;background:var(--ac)}\
#ai .btn{display:flex;align-items:center;justify-content:center;font-size:30px;font-weight:600;border-radius:999px;background:#111317;color:#fff;position:absolute;overflow:hidden}\
#ai .tag{white-space:normal!important;max-width:560px;font-size:26px;line-height:1.35;padding:10px 22px;border:1px solid var(--line);border-radius:999px;background:#fff;color:var(--ink2)}\
#ai .shot{border-radius:22px;border:1px solid var(--line);box-shadow:0 40px 80px rgba(17,19,23,.14);display:block}\
#ai .seg{display:flex;padding:6px;background:#fff;border:1px solid var(--line);border-radius:999px;font-size:30px;position:absolute}\
#ai .seg div{padding:10px 34px;border-radius:999px;position:relative;z-index:1}\
#ai .seg u{position:absolute;top:6px;bottom:6px;left:6px;width:142px;background:#111317;border-radius:999px;z-index:0}\
#ai .cur{position:absolute;left:0;top:0;width:48px;height:48px;pointer-events:none}\
#ai .rip{position:absolute;width:90px;height:90px;margin:-45px 0 0 -45px;border:3px solid var(--ac);border-radius:50%}\
#ai .skip{position:absolute;right:28px;bottom:26px;z-index:3;background:rgba(255,255,255,.8);border:1px solid var(--line);color:var(--ink2);border-radius:999px;padding:8px 18px;font:500 15px var(--sans);cursor:pointer}\
#ai .skip:hover{color:var(--ink)}\
#ai .skip:focus-visible{outline:2px solid var(--ac);outline-offset:3px}\
#ai .prog{position:absolute;left:0;bottom:0;height:3px;width:0;background:var(--ac);z-index:3}';

  /* ---------- DOM ---------- */
  var MARK = function (sz, k) { return '<svg data-k="' + k + '" width="' + sz + '" height="' + sz + '" viewBox="0 0 24 24"><path data-k="' + k + 'o" pathLength="1" d="M12 3 L22 20 H2 Z" fill="none" stroke="#16c784" stroke-width="1.7" stroke-linejoin="round" stroke-dasharray="1" stroke-dashoffset="0"/><path data-k="' + k + 'i" d="M12 10 L17 18.5 H7 Z" fill="#16c784"/></svg>'; };
  var ROWS = [
    ['Encrypted DNS', 'Requires system DNS lookups to be encrypted.'],
    ['Private network address', 'Uses a random hardware address on network connections.'],
    ['Generic device name', 'Uses the generic network name "arka" instead of yours.'],
    ['Temporary IPv6 addresses', 'Uses temporary IPv6 addresses that change over time.'],
    ['Sandboxed browser', 'Opens Firefox in a sandbox that sees only your Downloads folder.'],
    ['Firewall', 'Rejects connections other computers try to open to this one.']
  ];
  var TABS = ['How to encrypt your DNS', 'Change your MAC address', 'Is my firewall on?', 'Kernel settings, explained', 'sysctl.conf'];
  var PILE = [{ x: -300, y: -110, r: -7 }, { x: 250, y: -150, r: 5 }, { x: -140, y: 50, r: 3 }, { x: 290, y: 80, r: -4 }, { x: -20, y: -40, r: -1 }];

  var html = '<div class="bg" data-k="bg"></div>';
  html += '<div class="a" data-k="brand" style="left:120px;top:88px;display:flex;align-items:center;gap:16px;font-size:34px;font-weight:600">' + MARK(52, 'bm') + 'ArkaOS</div>';
  /* scene 1 */
  html += '<div data-k="s1">';
  PILE.forEach(function (p, i) {
    html += '<div class="a win0" data-k="w0' + i + '" style="left:' + (960 - 280) + 'px;top:' + (430 - 175) + 'px"><div class="tb"><u></u><u></u><u></u><span style="margin-left:14px">' + esc(TABS[i]) + '</span></div><i></i><i style="width:60%"></i><i></i><i style="width:75%"></i><i style="width:40%"></i></div>';
  });
  html += '<div class="a" data-k="dot" style="left:950px;top:510px;width:20px;height:20px;border-radius:50%;background:#111317"></div>';
  html += '<div class="a c serif big" data-k="h1" style="top:800px"></div></div>';
  /* scene 2 */
  html += '<div data-k="s2"><div class="a" data-k="ic1" style="left:880px;top:270px"><svg width="160" height="160" viewBox="0 0 80 80" fill="none" stroke="#111317" stroke-width="4" stroke-linecap="round"><circle data-k="ic1c" cx="34" cy="34" r="20" pathLength="1" stroke-dasharray="1" stroke-dashoffset="1"/><path data-k="ic1h" d="M49 49 L68 68" pathLength="1" stroke-dasharray="1" stroke-dashoffset="1"/></svg></div>';
  html += '<div class="a" data-k="ic2" style="left:880px;top:270px"><svg width="160" height="160" viewBox="0 0 80 80"><rect x="6" y="22" width="68" height="36" rx="18" data-k="ic2t" fill="#d8dce2"/><circle data-k="ic2k" cx="24" cy="40" r="13" fill="#fff" stroke="#111317" stroke-width="3"/></svg></div>';
  html += '<div class="a" data-k="ic3" style="left:880px;top:270px"><svg width="160" height="160" viewBox="0 0 80 80" fill="none" stroke="#111317" stroke-width="4" stroke-linecap="round"><circle cx="40" cy="40" r="28"/><path d="M31 32 Q31 22 40 22 Q50 22 50 31 Q50 37 43 40 Q40 42 40 47"/><circle cx="40" cy="57" r="1.5" fill="#111317"/></svg></div>';
  html += '<div class="a c serif big" data-k="t2" style="top:520px"></div></div>';
  /* scene 3 */
  html += '<div data-k="s3"><div class="a c" data-k="intro1" style="top:215px;font-size:52px;color:#4a505b">Introducing</div>';
  html += '<div class="a" data-k="bigmark" style="left:810px;top:300px">' + MARK(300, 'bg3') + '</div>';
  html += '<div class="a c" data-k="wm" style="top:660px;font-size:130px;font-weight:600;letter-spacing:-.03em">ArkaOS</div></div>';
  /* scene 4 + 5 */
  html += '<div data-k="s4"><div class="a c serif big" data-k="t4a" style="top:370px;font-size:112px"></div><div class="a c serif big" data-k="t4b" style="top:520px;font-size:112px"></div></div>';
  html += '<div data-k="s5"><div class="a c serif big" data-k="t5" style="top:330px;font-size:112px">Built on</div>';
  ['Fedora', 'bootc', 'KDE Plasma'].forEach(function (c, i) { html += '<div class="a chip" data-k="ch' + i + '" style="top:560px"><u></u>' + c + '</div>'; });
  html += '</div>';
  /* scene 6/7 window */
  html += '<div data-k="s6"><div class="a serif" data-k="h6" style="left:120px;top:330px;font-size:92px;line-height:1.08;letter-spacing:-.02em;font-weight:300"></div>';
  html += '<div class="a" data-k="h6s" style="left:124px;top:600px;font-size:34px;color:#4a505b;white-space:normal;width:520px;line-height:1.4"></div>';
  html += '<div class="a tag" data-k="h6t" style="left:124px;top:700px"></div>';
  html += '<div class="a win" data-k="win"><div class="wbar">' + MARK(30, 'wm2') + '<b data-k="wt">ArkaOS Setup</b><span data-k="wstep">Step 5 of 5</span></div><div class="wbody"><div class="side" data-k="side"><div class="on">Overview</div><div>Activity</div><div>Health</div><div>Updates &amp; recovery</div></div><div class="main"><h3 data-k="ptitle"></h3><p class="sub" data-k="psub"></p><div data-k="rows">';
  ROWS.forEach(function (r) { html += '<div class="row"><s></s><div><p>' + esc(r[0]) + '</p><em>' + esc(r[1]) + '</em></div><b class="st">Checking</b></div>'; });
  html += '</div><p class="note" data-k="pnote"></p></div></div></div></div>';
  /* scene 8 */
  html += '<div data-k="s8"><div class="a serif" data-k="h8" style="left:120px;top:370px;font-size:80px;line-height:1.1;letter-spacing:-.02em;font-weight:300"></div>';
  html += '<div class="a tag" data-k="h8t" style="left:124px;top:660px">Tested in a VM</div>';
  html += '<div class="a card" data-k="cA"><small data-k="cAs"></small><h4 data-k="cAh"></h4><span data-k="cAp"></span></div>';
  html += '<div class="a card" data-k="cB"><small data-k="cBs"></small><h4 data-k="cBh"></h4><span data-k="cBp"></span><div class="bar"><i data-k="cBb"></i></div></div>';
  html += '<div class="a btn" data-k="rb" style="left:1000px;top:730px;width:520px;height:84px">Restart when you are ready</div></div>';
  /* scene 9 / 10 / montage */
  html += '<div data-k="s9"><div class="a serif" data-k="h9" style="left:120px;top:64px;font-size:84px;font-weight:300;letter-spacing:-.02em"></div><div class="seg" data-k="seg" style="left:1330px;top:70px"><u data-k="segu"></u><div>Light</div><div>Dark</div></div><div class="a" data-k="im9w" style="left:288px;top:190px;width:1344px;height:840px"><img class="shot" data-k="im9" src="img/intro-desktop-light.png" alt="" style="position:absolute;inset:0;width:100%;height:100%"><img class="shot" data-k="im9d" src="img/intro-desktop-dark.png" alt="" style="position:absolute;inset:0;width:100%;height:100%;opacity:0"></div></div>';
  html += '<div data-k="s10"><div class="a c serif big" data-k="h10" style="top:150px;font-size:100px"></div><img class="a shot" data-k="im10" src="img/intro-dock-light.png" alt="" style="left:380px;top:450px;width:1160px;height:188px;border:1px solid #dfe3e8;box-shadow:0 30px 60px rgba(17,19,23,.12);border-radius:22px"></div>';
  html += '<div data-k="s11"><img class="a shot" data-k="m0" src="img/setup-privacy-light.png" alt="" style="left:180px;top:70px;width:1560px;height:975px"><img class="a shot" data-k="m1" src="img/dashboard-light.png" alt="" style="left:180px;top:70px;width:1560px;height:975px"><img class="a shot" data-k="m2" src="img/intro-desktop-light.png" alt="" style="left:180px;top:70px;width:1560px;height:975px"><img class="a shot" data-k="m3" src="img/intro-dock-light.png" alt="" style="left:230px;top:420px;width:1460px;height:237px;border:1px solid #dfe3e8;box-shadow:0 30px 60px rgba(17,19,23,.12);border-radius:22px"></div>';
  /* scene 12/13 */
  html += '<div data-k="s12"><div class="a" data-k="mk12" style="left:810px;top:230px">' + MARK(300, 'bg12') + '</div><div class="a c serif big" data-k="h12a" style="top:600px"></div><div class="a c serif big" data-k="h12b" style="top:730px"></div></div>';
  html += '<div data-k="s13"><div class="a" data-k="mk13" style="left:900px;top:200px">' + MARK(120, 'bg13') + '</div>';
  html += '<a class="a btn" data-k="pill" href="#try" style="left:640px;top:410px;width:640px;height:104px;background:#fff;color:#111317;border:1px solid #dfe3e8;box-shadow:0 18px 40px rgba(17,19,23,.10);text-decoration:none"><span data-k="pfill" style="position:absolute;left:0;top:0;bottom:0;width:0;background:#16c784"></span><span style="position:relative;font-size:38px">Try it in a virtual machine</span></a>';
  html += '<div class="a c" data-k="end1" style="top:590px;font-size:36px;color:#4a505b"></div></div>';
  html += '<div data-k="rips"></div><svg class="cur" data-k="cur" viewBox="0 0 32 32"><path d="M5 3 L5 27 L11 21.5 L15 30 L19 28.2 L15 19.8 L23 19.5 Z" fill="#111317" stroke="#fff" stroke-width="2" stroke-linejoin="round"/></svg>';

function launch() {
  if (active) return; active = true;
  /* ---------- mount ---------- */
  var opener = document.activeElement;
  var host = document.createElement('div');
  host.style.cssText = 'position:fixed;inset:0;z-index:99999';
  var shadow = host.attachShadow({ mode: 'open' });
  var root = document.createElement('div');
  root.id = 'ai'; root.setAttribute('role', 'dialog'); root.setAttribute('aria-modal', 'true'); root.setAttribute('aria-label', 'ArkaOS intro');
  var st = document.createElement('style'); st.textContent = css; root.appendChild(st);
  var stage = document.createElement('div'); stage.className = 's'; stage.innerHTML = html; root.appendChild(stage);
  var skip = document.createElement('button'); skip.className = 'skip'; skip.type = 'button'; skip.textContent = 'Skip intro (Esc)'; root.appendChild(skip);
  var prog = document.createElement('div'); prog.className = 'prog'; root.appendChild(prog);
  var htmlEl = document.documentElement, prevOverflow = htmlEl.style.overflow;
  htmlEl.style.overflow = 'hidden';
  shadow.appendChild(root);
  document.body.appendChild(host);

  var K = {}; [].forEach.call(stage.querySelectorAll('[data-k]'), function (n) { K[n.getAttribute('data-k')] = n; });
  var rowEls = [].slice.call(K.rows.children);
  var scenes = ['s1', 's2', 's3', 's4', 's5', 's6', 's8', 's9', 's10', 's11', 's12', 's13'];

  function fit() {
    var w = root.clientWidth, h = root.clientHeight, sc = Math.min(w / W, h / H);
    stage.style.transform = 'translate(' + ((w - W * sc) / 2) + 'px,' + ((h - H * sc) / 2) + 'px) scale(' + sc + ')';
  }
  addEventListener('resize', fit); fit();

  /* ---------- element setters ---------- */
  var shown = {};
  function vis(k, on) { if (shown[k] !== on) { shown[k] = on; K[k].style.display = on ? '' : 'none'; } }
  function put(k, o) {
    var e = K[k], tr = '';
    if (o.x !== undefined || o.y !== undefined) tr += 'translate(' + (o.x || 0) + 'px,' + (o.y || 0) + 'px) ';
    if (o.r) tr += 'rotate(' + o.r + 'deg) ';
    if (o.s !== undefined) tr += 'scale(' + o.s + ') ';
    e.style.transform = tr;
    if (o.o !== undefined) e.style.opacity = o.o;
    e.style.filter = o.b ? 'blur(' + o.b + 'px)' : '';
  }
  function rise(k, p, dy) { put(k, { y: (1 - eo(p)) * (dy === undefined ? 40 : dy), o: eo(p), b: (1 - eo(p)) * 10 }); }
  function typed(k, str, p, hold, t) {
    var n = Math.round(str.length * clamp(p)), live = p > 0 && p < 1 || (hold && Math.floor(t * 2) % 2 === 0 && p > 0);
    K[k].innerHTML = esc(str.slice(0, n)) + (live ? '<i class="cr"></i>' : '');
  }
  /* html lines with a bold tail: [light, bold] */
  function typed2(k, a, b, p, t, hold) {
    var s = a + b, n = Math.round(s.length * clamp(p)), la = Math.min(n, a.length), lb = Math.max(0, n - a.length);
    var live = p > 0 && p < 1 || (hold && Math.floor(t * 2) % 2 === 0 && p > 0);
    K[k].innerHTML = esc(a.slice(0, la)) + (lb ? '<b>' + esc(b.slice(0, lb)) + '</b>' : '') + (live ? '<i class="cr"></i>' : '');
  }
  function draw(k, p) { K[k].style.strokeDashoffset = 1 - clamp(p); }
  function swapText(k, list, t, t0, per) { /* in-place word swap; returns nothing, sets html */
    var i = Math.min(list.length - 1, Math.floor((t - t0) / per)), local = (t - t0) - i * per;
    var pin = seg(local, 0, .38), pout = (i < list.length - 1) ? seg(local, per - .22, per) : 0;
    K[k].innerHTML = list[i]; put(k, { y: (1 - eo(pin)) * 34 - eo(pout) * 30, o: eo(pin) * (1 - pout), b: (1 - eo(pin)) * 9 + pout * 9 });
  }

  /* ---------- cursor ---------- */
  var SEGS = [
    { a: 1.5, b: 2.3, x0: 1760, y0: 200, x1: 990, y1: 440 },
    { a: 2.5, b: 3.7, x0: 990, y0: 440, x1: 950, y1: 520 },
    { a: 26.5, b: 27.5, x0: 1700, y0: 900, x1: 1260, y1: 772 },
    { a: 39.4, b: 40.4, x0: 1500, y0: 320, x1: 1040, y1: 470 }
  ];
  var CLICKS = [2.35, 27.6, 40.5];
  var held = { 0: [1.5, 3.7], 2: [26.5, 28.4], 3: [39.4, 41.4] };
  function cursor(t) {
    var vis0 = false, x = 0, y = 0, down = false;
    SEGS.forEach(function (s, i) {
      if (t >= s.a - .05 && t <= s.b + (i === 1 ? .25 : .9)) {
        vis0 = true; var p = eio(seg(t, s.a, s.b));
        x = lerp(s.x0, s.x1, p); y = lerp(s.y0, s.y1, p) - Math.sin(p * Math.PI) * 60;
        if (t > s.b) { x = s.x1; y = s.y1; }
      }
    });
    CLICKS.forEach(function (c) { if (t >= c && t < c + .18) down = true; });
    var fade = 1;
    if (t > 3.5 && t < 4) fade = 1 - seg(t, 3.7, 4.0);
    K.cur.style.opacity = vis0 ? fade : 0;
    K.cur.style.transform = 'translate(' + x + 'px,' + y + 'px) scale(' + (down ? .84 : 1) + ')';
    K.cur.style.transformOrigin = '5px 3px';
    var rh = '';
    CLICKS.forEach(function (c, i) {
      var p = seg(t, c, c + .7);
      if (p > 0 && p < 1) {
        var pos = i === 0 ? [990, 440] : i === 1 ? [1260, 772] : [1040, 470];
        rh += '<div class="rip" style="left:' + pos[0] + 'px;top:' + pos[1] + 'px;opacity:' + (1 - p) * .8 + ';transform:scale(' + (.3 + p * 1.2) + ')"></div>';
      }
    });
    K.rips.innerHTML = rh;
  }

  /* ---------- window rows ---------- */
  function rowsAt(t, t0, per, instant) {
    rowEls.forEach(function (r, i) {
      var a = instant ? 1 : seg(t, t0 + i * .06, t0 + i * .06 + .4);
      var tick = instant ? 1 : seg(t, t0 + 1.1 + i * per, t0 + 1.1 + i * per + .01);
      r.style.opacity = eo(a); r.style.transform = 'translateY(' + (1 - eo(a)) * 24 + 'px)';
      r.firstChild.style.background = tick ? '#16c784' : '#cfd5dc';
      var s = r.lastChild; s.textContent = tick ? 'Configured' : 'Checking'; s.style.color = tick ? '#0b7a53' : '#9aa2ad';
    });
  }

  /* ---------- render ---------- */
  function render(t) {
    var on = {};
    function s(k, a, b) { var v = t >= a && t < b; on[k] = v; return v; }
    var bgx = Math.sin(t / 5) * 60, bgy = Math.cos(t / 7) * 40;
    K.bg.style.transform = 'translate(' + bgx + 'px,' + bgy + 'px)';

    /* persistent small brand mark: 9 to 30 */
    var brandIn = seg(t, 9.0, 9.7) * (1 - seg(t, 29.7, 30.0)); vis('brand', brandIn > 0);
    if (brandIn > 0) { K.brand.style.opacity = brandIn; draw('bmo', 1); }

    /* 1: the old way 0-4 */
    if (s('s1', 0, 4.1)) {
      var drag = eio(seg(t, 2.5, 3.7)), grab = t > 2.35;
      PILE.forEach(function (p, i) {
        var a = seg(t, .1 + i * .22, .1 + i * .22 + .55), sh = 1 - drag;
        var cx = lerp(0, 980 - 960 + (t > 2.5 ? lerp(0, -30, drag) : 0), drag);
        put('w0' + i, { x: lerp(p.x, 0, drag) + cx, y: lerp(p.y + (1 - eo(a)) * 80, 90 * drag, drag) , r: p.r * sh, s: (0.6 + 0.4 * back(a)) * Math.max(.02, sh), o: eo(a) * (drag > .96 ? 0 : 1) });
      });
      var dotp = seg(t, 3.45, 3.7); put('dot', { s: dotp, o: dotp * (1 - seg(t, 3.9, 4.1)) });
      var hp = seg(t, .3, 1.7); typed2('h1', 'Privacy, ', 'by hand.', hp, t, false);
      put('h1', { o: 1 - seg(t, 2.6, 3.1), y: -seg(t, 2.6, 3.1) * 40, b: seg(t, 2.6, 3.1) * 10 });
    }
    /* 2: three chores 4-7 */
    if (s('s2', 3.95, 7.1)) {
      var words = ['Look it up.', 'Change it.', 'Hope it stayed.'], t0 = 4.0, per = 1.05;
      swapText('t2', words.map(function (w) { return '<b>' + w + '</b>'; }), t, t0, per);
      var idx = Math.min(2, Math.floor((t - t0) / per));
      [1, 2, 3].forEach(function (n) {
        var act = idx === n - 1, lt = (t - t0) - (n - 1) * per, p = act ? seg(lt, 0, .38) : 0, po = act && n < 3 ? seg(lt, per - .22, per) : 0;
        if (t < t0) { p = 0; }
        put('ic' + n, { y: (1 - eo(p)) * 40 - eo(po) * 30, o: act ? eo(p) * (1 - po) : 0, b: (1 - eo(p)) * 8 });
      });
      draw('ic1c', seg(t, 4.1, 4.7)); draw('ic1h', seg(t, 4.55, 4.95));
      var tog = seg(t, 5.2, 5.6); K.ic2k.setAttribute('cx', lerp(24, 56, eo(tog))); K.ic2t.setAttribute('fill', tog > .5 ? '#16c784' : '#d8dce2');
      var out = seg(t, 6.85, 7.1); K.s2.style.opacity = 1 - out;
    }
    /* 3: logo 7-9 */
    if (s('s3', 6.85, 9.1)) {
      var bl = seg(t, 7.0, 7.9);
      draw('bg3o', bl); K.bg3i.style.opacity = seg(t, 7.7, 8.2); K.bg3i.style.transformOrigin = '12px 15px'; K.bg3i.style.transform = 'scale(' + (.4 + .6 * back(seg(t, 7.7, 8.3))) + ')';
      put('intro1', { y: (1 - eo(seg(t, 7.4, 8.0))) * 30, o: eo(seg(t, 7.4, 8.0)), b: (1 - eo(seg(t, 7.4, 8.0))) * 9 });
      var wp = seg(t, 7.9, 8.5); K.wm.style.opacity = eo(wp); K.wm.style.transform = 'translateY(' + (1 - eo(wp)) * 30 + 'px)';
      var sw = seg(t, 8.2, 9.0); K.wm.style.backgroundImage = 'linear-gradient(100deg,#111317 ' + (sw * 140 - 50) + '%,#16c784 ' + (sw * 140 - 30) + '%,#111317 ' + (sw * 140 - 10) + '%)';
      K.wm.style.webkitBackgroundClip = K.wm.style.backgroundClip = 'text'; K.wm.style.color = 'transparent'; K.wm.style.webkitTextFillColor = 'transparent';
      var shrink = eo(seg(t, 8.6, 9.2)); K.bigmark.style.transformOrigin = '150px 150px';
      put('bigmark', { x: -shrink * (810 + 150 - 146), y: -shrink * (300 + 150 - 114), s: 1 - shrink * .8 });
      var fo = seg(t, 8.5, 9.05); K.intro1.style.opacity *= 1 - fo; K.wm.style.opacity = (1 - fo) * eo(wp);
    }
    /* 4: a Linux desktop 9-12 */
    if (s('s4', 9.0, 12.2)) {
      rise('t4a', seg(t, 9.1, 9.7));
      K.t4a.innerHTML = 'A Linux desktop that';
      var ph = ['sets up your <b class="acc">privacy.</b>', 'tells you <b class="acc">where it stands.</b>', 'keeps your <b class="acc">last version.</b>'];
      swapText('t4b', ph, t, 9.5, 0.95);
      var o4 = seg(t, 11.95, 12.2); K.s4.style.opacity = 1 - o4;
    }
    /* 5: built on 12-15 */
    if (s('s5', 11.95, 15.1)) {
      rise('t5', seg(t, 12.1, 12.7));
      var wds = [112, 112, 112], widths = [270, 220, 330], gap = 28, total = 270 + 220 + 330 + gap * 2, x0 = (1920 - total) / 2, acc = 0;
      ['ch0', 'ch1', 'ch2'].forEach(function (k, i) {
        var p = seg(t, 12.8 + i * .35, 13.5 + i * .35); K[k].style.left = (x0 + acc) + 'px'; K[k].style.width = widths[i] + 'px'; K[k].style.justifyContent = 'center'; acc += widths[i] + gap;
        put(k, { y: (1 - back(p)) * 40, s: .8 + .2 * back(p), o: eo(Math.min(1, p * 1.5)) });
      });
      var o5 = seg(t, 14.8, 15.1); K.s5.style.opacity = 1 - o5;
    }
    /* 6/7: setup then dashboard window 15-25 */
    if (s('s6', 14.9, 25.1)) {
      var dash = t >= 21;
      var win = seg(t, 15.0, 15.9), pushin = 1 + seg(t, 15.9, 25) * .05;
      K.win.style.perspective = ''; K.win.style.transform = 'perspective(2400px) rotateY(' + (-(1 - eo(win)) * 22) + 'deg) rotateX(' + ((1 - eo(win)) * 10) + 'deg) translateY(' + ((1 - eo(win)) * 60) + 'px) scale(' + pushin + ')';
      K.win.style.opacity = eo(Math.min(1, win * 1.5));
      var sidew = dash ? eo(seg(t, 21.0, 21.8)) * 236 : 0;
      K.side.style.width = sidew + 'px';
      K.wt.textContent = dash ? 'Privacy Dashboard' : 'ArkaOS Setup'; K.wstep.textContent = dash ? '' : 'Step 5 of 5';
      K.ptitle.textContent = dash ? 'Privacy' : 'Your privacy settings';
      K.psub.textContent = dash ? "What ArkaOS has set up on this computer. Covers only what's listed here." : "ArkaOS sets these up on its own. Here's how they stand right now.";
      rowsAt(t, 15.5, .3, dash);
      var noteP = seg(t, 18.0, 18.5);
      if (!dash) { K.pnote.textContent = "Checked just now. This shows what ArkaOS has set up, and covers only what's listed here."; K.pnote.style.opacity = eo(noteP); }
      else { var secs = Math.round(44 * seg(t, 21.8, 24.2)); K.pnote.textContent = 'Checked ' + secs + ' s ago.'; K.pnote.style.opacity = 1; }
      var outw = seg(t, 24.75, 25.1); K.win.style.opacity *= 1 - outw;
      /* headline */
      var h = dash ? ['Plain answers,\n', 'no score.'] : ['Settings,\n', 'already on.'];
      var hp1 = dash ? seg(t, 21.1, 22.0) : seg(t, 15.2, 16.3);
      var a1 = h[0].replace('\n', '<br>');
      var n = Math.round((h[0].length + h[1].length) * hp1), la = Math.min(n, h[0].length), lb = Math.max(0, n - h[0].length);
      K.h6.innerHTML = esc(h[0].slice(0, la)).replace('\n', '<br>') + (lb ? '<b style="font-weight:600">' + esc(h[1].slice(0, lb)) + '</b>' : '') + ((hp1 > 0 && hp1 < 1) ? '<i class="cr"></i>' : '');
      var swapOut = dash ? 0 : seg(t, 20.5, 20.95);
      K.h6.style.opacity = 1 - swapOut * (dash ? 0 : 1); K.h6.style.filter = swapOut ? 'blur(' + swapOut * 8 + 'px)' : '';
      K.h6s.textContent = dash ? '' : 'Setup shows where each setting stands before you start.';
      K.h6s.style.opacity = dash ? 0 : eo(seg(t, 16.6, 17.3)) * (1 - swapOut);
      K.h6t.textContent = 'Configured or Needs attention. Never a score.';
      var tg = dash ? seg(t, 22.4, 23.1) : 0; K.h6t.style.opacity = eo(tg); K.h6t.style.transform = 'translateY(' + (1 - eo(tg)) * 24 + 'px)';
      K.s6.style.opacity = 1;
    }
    /* 8: updates 25-30 */
    if (s('s8', 24.9, 30.1)) {
      var tl = t;
      var sw1 = tl < 28.4 ? 0 : 1; /* phase: 0 new update, 1 failed */
      var hh = tl < 28.55 ? ['Updates\n', 'wait for you.'] : ['Broken update?\n', 'Back, by itself.'];
      var hpp = tl < 28.55 ? seg(tl, 25.0, 25.9) : seg(tl, 28.65, 29.5);
      var nn = Math.round((hh[0].length + hh[1].length) * hpp), a2 = Math.min(nn, hh[0].length), b2 = Math.max(0, nn - hh[0].length);
      K.h8.innerHTML = esc(hh[0].slice(0, a2)).replace('\n', '<br>') + (b2 ? '<b style="font-weight:600">' + esc(hh[1].slice(0, b2)) + '</b>' : '') + ((hpp > 0 && hpp < 1) ? '<i class="cr"></i>' : '');
      var sOut = seg(tl, 28.2, 28.55); K.h8.style.opacity = tl < 28.55 ? 1 - sOut : 1; K.h8.style.filter = tl < 28.55 && sOut ? 'blur(' + sOut * 8 + 'px)' : '';
      /* card slots */
      var ap = seg(tl, 25.2, 25.9), bp = seg(tl, 25.8, 26.5);
      var swap1 = eo(seg(tl, 27.75, 28.3)), swap2 = eo(seg(tl, 29.0, 29.55));
      var slotA = 250, slotB = 490;
      var ay = lerp(lerp(slotA, slotB, swap1), slotA, swap2), by = lerp(lerp(slotB, slotA, swap1), slotB, swap2);
      K.cA.style.top = ay + 'px'; K.cB.style.top = by + 'px';
      var lat = function (sw) { return Math.sin(Math.PI * sw) * 70; };
      put('cA', { x: lat(seg(tl, 27.75, 28.3)) + lat(seg(tl, 29.0, 29.55)), y: (1 - eo(ap)) * 50, o: eo(ap) * (tl > 27.75 && tl < 29.0 ? .85 : 1) });
      put('cB', { x: -lat(seg(tl, 27.75, 28.3)) - lat(seg(tl, 29.0, 29.55)), y: (1 - eo(bp)) * 70, o: eo(bp) });
      var phase = tl < 27.75 ? 0 : tl < 29.0 ? 1 : 2;
      K.cAs.textContent = phase === 0 ? 'Running now' : phase === 1 ? 'Previous version' : 'Back on the last version';
      K.cAh.textContent = 'Current version';
      K.cAp.textContent = phase === 1 ? 'Kept on disk, ready to go back to.' : phase === 2 ? 'Tried the new one three times first.' : 'Stays on disk after the update.';
      K.cBs.textContent = phase === 0 ? 'New version' : phase === 1 ? 'Running now' : 'Failed its start-up checks';
      K.cBs.style.color = phase === 2 ? '#a35a00' : '';
      K.cBh.textContent = 'New version';
      K.cBp.textContent = phase === 0 ? 'Prepared next to the one you use.' : phase === 1 ? 'Took over when you restarted.' : 'Went back to the previous version.';
      K.cBb.style.width = (phase === 0 ? eo(seg(tl, 26.0, 27.0)) * 100 : 100) + '%'; K.cBb.style.background = phase === 2 ? '#c98a2b' : '#16c784';
      var rbp = seg(tl, 26.9, 27.4) * (1 - seg(tl, 27.9, 28.2));
      put('rb', { y: (1 - eo(seg(tl, 26.9, 27.4))) * 30, o: rbp, s: tl >= 27.6 && tl < 27.78 ? .95 : 1 });
      K.h8t.style.opacity = eo(seg(tl, 29.3, 29.8)); K.h8t.style.display = '';
      K.s8.style.opacity = 1 - seg(tl, 29.85, 30.1);
    }
    /* 9: light or dark 30-32.5 */
    if (s('s9', 29.95, 32.6)) {
      typed('h9', 'Light or dark. You choose.', seg(t, 30.1, 30.9), false, t);
      var sp = eo(seg(t, 30.9, 31.25)); K.segu.style.transform = 'translateX(' + (sp * 150) + 'px)'; K.seg.children[1].style.color = sp < .5 ? '#fff' : ''; K.seg.children[2].style.color = sp >= .5 ? '#fff' : '';
      var im = seg(t, 30.0, 30.7); put('im9w', { y: (1 - eo(im)) * 80, s: 1 + seg(t, 30.7, 32.6) * .03, o: eo(im) }); K.im9d.style.opacity = eo(seg(t, 31.1, 31.5));
      K.s9.style.opacity = 1 - seg(t, 32.4, 32.6);
    }
    /* 10: dock 32.5-34.5 */
    if (s('s10', 32.4, 34.6)) {
      typed2('h10', 'Your apps, ', 'one dock.', seg(t, 32.6, 33.3), t, false);
      var d = seg(t, 32.55, 33.1); put('im10', { y: (1 - eo(d)) * 70, s: 1 + seg(t, 33.1, 34.5) * .1, o: eo(d) });
      K.s10.style.opacity = 1 - seg(t, 34.4, 34.6);
    }
    /* 11: montage 34.5-36.5 */
    if (s('s11', 34.45, 36.6)) {
      var ids = ['m0', 'm1', 'm2', 'm3'];
      ids.forEach(function (id, i) {
        var a = 34.5 + i * .5, vs = t >= a && t < a + .5; K[id].style.display = vs ? '' : 'none';
        if (vs) { var p = seg(t, a, a + .5); K[id].style.transform = 'scale(' + (1 + p * .08) + ')'; K[id].style.opacity = eo(seg(t, a, a + .12)); }
      });
      K.s11.style.opacity = t > 36.4 ? 1 - seg(t, 36.4, 36.6) : 1;
    } else { ['m0', 'm1', 'm2', 'm3'].forEach(function (id) { K[id].style.display = 'none'; }); }
    /* 12: collapse and line 36.5-38.5 */
    if (s('s12', 36.4, 38.6)) {
      var mp = seg(t, 36.5, 37.2); draw('bg12o', mp); K.bg12i.style.opacity = mp;
      put('mk12', { s: .5 + .5 * back(mp), o: eo(Math.min(1, mp * 2)) });
      K.mk12.style.transformOrigin = '150px 150px';
      typed2('h12a', 'Private by setup.', '', seg(t, 37.0, 37.7), t, false);
      K.h12b.innerHTML = ''; typed2('h12b', '', 'Honest about it.', seg(t, 37.5, 38.2), t, false);
      K.h12b.className = 'a c serif big acc';
      var o12 = seg(t, 38.35, 38.6); K.s12.style.opacity = 1 - o12;
    }
    /* 13: end card 38.5-43 */
    if (s('s13', 38.5, DUR + .1)) {
      var mk = seg(t, 38.6, 39.2); draw('bg13o', mk); K.bg13i.style.opacity = mk;
      var pl = seg(t, 38.8, 39.4); put('pill', { y: (1 - eo(pl)) * 40, o: eo(pl) });
      K.pfill.style.width = (eo(seg(t, 39.5, 40.3)) * 100) + '%';
      typed('end1', 'Developer preview. Tested in a VM. Not an installer yet.', seg(t, 40.7, 42.1), true, t);
      K.s13.style.opacity = 1 - seg(t, DUR - .9, DUR - .5);
    }
    scenes.forEach(function (k) { vis(k, !!on[k]); });
    vis('s6', !!on.s6);
    cursor(t);
    prog.style.width = (t / DUR * 100) + '%';
    var fadeOut = seg(t, DUR - .5, DUR); root.style.opacity = 1 - fadeOut;
  }

  /* ---------- player ---------- */
  var raf = 0, last = 0, now = 0, done = false;
  function finish() {
    if (done) return; done = true; cancelAnimationFrame(raf);
    htmlEl.style.overflow = prevOverflow; host.remove(); removeEventListener('keydown', onKey);
    active = false; if (opener && opener.focus) opener.focus();
  }
  function onKey(e) { if (e.key === 'Escape') finish(); }
  function loop(ts) {
    if (!last) last = ts; var dt = Math.min(.1, (ts - last) / 1000); last = ts; now += dt;
    if (now >= DUR) { finish(); return; }
    render(now); raf = requestAnimationFrame(loop);
  }
  skip.addEventListener('click', finish);
  K.pill.addEventListener('click', function () { finish(); });
  addEventListener('keydown', onKey);
  window.ArkaIntro = { seek: function (t) { now = t; render(t); }, duration: DUR, skip: finish };
  render(0);
  if (mode !== 'seek') {
    var imgs = [].slice.call(stage.querySelectorAll('img')), go = false;
    var start = function () { if (!go && !done) { go = true; skip.focus(); raf = requestAnimationFrame(loop); } };
    Promise.all(imgs.map(function (i) { return i.complete ? 1 : new Promise(function (r) { i.onload = i.onerror = r; }); })).then(start);
    setTimeout(start, 2500);
  }
}
  var active = false;
  document.addEventListener('click', function (e) {
    var t = e.target.closest && e.target.closest('[data-film]');
    if (t) { e.preventDefault(); launch(); }
  });
  if (mode === 'seek') launch();
})();
