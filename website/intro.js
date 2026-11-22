/* The ArkaOS film, "One spark" (84 s, with sound). It plays only when the visitor asks for it
   (any element with data-film), in a full-window player with a Close button. Esc closes it.
   The film is a video made from the Remotion source in the project; nothing loads from other sites. */
(function () {
  'use strict';
  var SRC = 'media/onespark.mp4', POSTER = 'img/onespark-poster.jpg';
  var reduce = !!(window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches);
  var open = null;

  var css = '\
:host{all:initial}\
.o{position:fixed;inset:0;z-index:99999;background:#06070b;display:flex;align-items:center;justify-content:center;font-family:Inter,system-ui,sans-serif}\
video{width:100%;height:100%;max-height:100vh;object-fit:contain;background:#06070b;outline:none}\
button{position:absolute;top:16px;right:16px;height:40px;padding:0 18px;border-radius:20px;border:1px solid rgba(255,255,255,.22);background:rgba(20,24,28,.72);color:#ecf1ef;font:500 15px/1 Inter,system-ui,sans-serif;cursor:pointer}\
button:hover{background:rgba(40,46,52,.9)}\
button:focus-visible{outline:2px solid #4db491;outline-offset:2px}';

  function launch(opener) {
    if (open) return;
    var host = document.createElement('div');
    var shadow = host.attachShadow({ mode: 'open' });
    shadow.innerHTML = '<style>' + css + '</style>' +
      '<div class="o" role="dialog" aria-modal="true" aria-label="The ArkaOS film">' +
      '<video playsinline controls preload="auto" poster="' + POSTER + '">' +
      '<source src="' + SRC + '" type="video/mp4">' +
      'Your browser can’t play this film here.</video>' +
      '<button type="button">Close (Esc)</button></div>';
    document.body.appendChild(host);
    var video = shadow.querySelector('video'), close = shadow.querySelector('button');
    var html = document.documentElement, prevOverflow = html.style.overflow;
    html.style.overflow = 'hidden';

    function finish() {
      if (!open) return;
      video.pause(); host.remove(); html.style.overflow = prevOverflow;
      removeEventListener('keydown', onKey); open = null;
      if (opener && opener.focus) opener.focus();
    }
    function onKey(e) {
      if (e.key === 'Escape') finish();
      else if (e.key === 'Tab') { e.preventDefault(); (shadow.activeElement === close ? video : close).focus(); }
    }
    close.addEventListener('click', finish);
    video.addEventListener('ended', finish);
    addEventListener('keydown', onKey);
    open = finish;
    close.focus();
    // The visitor asked for the film, so it starts at once, with sound. With reduced motion it
    // waits for them to press play.
    if (!reduce) {
      var p = video.play();
      if (p && p.catch) p.catch(function () { /* the browser wants another click; the controls are there */ });
    }
  }

  document.addEventListener('click', function (e) {
    var t = e.target.closest && e.target.closest('[data-film]');
    if (t) { e.preventDefault(); launch(t); }
  });
})();
