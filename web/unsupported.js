// One feature check picks the variant: WebGPU present means the adapter failed. Without script
// the page reads as the no-WebGPU variant, the common case. The status line names the browser.
(function () {
  var adapter = "gpu" in navigator && !!navigator.gpu;
  if (adapter) {
    document.documentElement.classList.add("is-adapter");
    document.querySelector(".card").setAttribute("aria-labelledby", "h-adapter");
  }
  var ua = navigator.userAgent, m;
  var name =
    (m = ua.match(/Edg\/(\d+)/)) ? "Edge " + m[1] :
    (m = ua.match(/Firefox\/(\d+)/)) ? "Firefox " + m[1] :
    (m = ua.match(/(?:Chrome|CriOS)\/(\d+)/)) ? "Chrome " + m[1] :
    (m = ua.match(/Version\/(\d+)[\d.]* (?:Mobile\/\S+ )?Safari/)) ? "Safari " + m[1] : "";
  var os = /iPhone|iPad|iPod/.test(ua) ? "iOS" : /Android/.test(ua) ? "Android" : /Mac OS X/.test(ua) ? "macOS" :
    /Windows/.test(ua) ? "Windows" : /CrOS/.test(ua) ? "ChromeOS" : /Linux/.test(ua) ? "Linux" : "";
  if (!name) return;
  var label = os ? name + " on " + os : name;
  document.querySelectorAll("[data-browser]").forEach(function (b) { b.textContent = label; });
})();
