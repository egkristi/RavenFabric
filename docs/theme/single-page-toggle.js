// Adds a "Single Page" / "Multi Page" toggle to the mdBook toolbar
(function () {
    "use strict";

    var rightButtons = document.querySelector(".right-buttons");
    if (!rightButtons) return;

    var toggle = document.createElement("a");
    toggle.className = "icon-button";
    toggle.title = "View as single page";
    toggle.setAttribute("aria-label", "View as single page");

    var isPrintPage = window.location.pathname.endsWith("/print.html");
    var baseUrl = document.querySelector("meta[name='site-url']");
    var siteRoot = baseUrl ? baseUrl.getAttribute("content") : "/docs/";

    if (isPrintPage) {
        toggle.href = siteRoot;
        toggle.title = "View as multi-page";
        toggle.setAttribute("aria-label", "View as multi-page");
        toggle.innerHTML = '<i class="fa fa-files-o"></i>';
    } else {
        toggle.href = siteRoot + "print.html";
        toggle.innerHTML = '<i class="fa fa-file-text-o"></i>';
    }

    rightButtons.insertBefore(toggle, rightButtons.firstChild);
})();
