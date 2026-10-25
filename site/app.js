// Fills in the newest release number from GitHub. Without it the links still work.
(function () {
  var els = document.querySelectorAll("[data-version]");
  if (!els.length || !window.fetch) return;
  fetch("https://api.github.com/repos/nreed97/QRZero/releases?per_page=1")
    .then(function (r) { return r.ok ? r.json() : []; })
    .then(function (list) {
      if (!list.length) return;
      var m = /(\d+\.\d+(\.\d+)?)/.exec(list[0].tag_name);
      if (!m) return;
      els.forEach(function (e) { e.textContent = "v" + m[1]; });
    })
    .catch(function () {});
})();

// Demo clips play only while on screen, one screenful at a time. With reduced motion or Data Saver on
// they stay on their still picture and get play controls instead.
(function () {
  var clips = document.querySelectorAll("video.clip");
  if (!clips.length) return;
  var still = (window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches) ||
    (navigator.connection && navigator.connection.saveData);
  if (still || !("IntersectionObserver" in window)) {
    clips.forEach(function (v) { v.controls = true; v.loop = false; });
    return;
  }
  var seen = new IntersectionObserver(function (entries) {
    entries.forEach(function (e) {
      var v = e.target;
      if (e.isIntersecting) { var p = v.play(); if (p && p.catch) p.catch(function () {}); }
      else v.pause();
    });
  }, { threshold: 0.6 });
  clips.forEach(function (v) { seen.observe(v); });
})();
