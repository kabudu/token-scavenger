"use strict";
const navToggle = document.querySelector("[data-mobile-toggle]");
const mobileMenu = document.querySelector("[data-mobile-nav]");
if (navToggle && mobileMenu) {
    navToggle.addEventListener("click", () => {
        const expanded = navToggle.getAttribute("aria-expanded") === "true";
        navToggle.setAttribute("aria-expanded", String(!expanded));
        mobileMenu.classList.toggle("hidden");
    });
}
const closeMenuLinks = document.querySelectorAll("[data-close-menu]");
closeMenuLinks.forEach((link) => {
    link.addEventListener("click", () => {
        if (mobileMenu && navToggle) {
            mobileMenu.classList.add("hidden");
            navToggle.setAttribute("aria-expanded", "false");
        }
    });
});
const installTabs = document.querySelectorAll("[data-install-tab]");
const installPanels = document.querySelectorAll("[data-install-panel]");
const activeInstallTabClass = "min-w-24 flex-1 rounded-xl bg-orange-500 px-4 py-3 text-sm font-semibold text-slate-950 transition";
const inactiveInstallTabClass = "min-w-24 flex-1 rounded-xl px-4 py-3 text-sm font-semibold text-slate-300 transition hover:bg-white/5 hover:text-white";
installTabs.forEach((tab) => {
    tab.addEventListener("click", () => {
        const selected = tab.dataset.installTab;
        installTabs.forEach((candidate) => {
            const isSelected = candidate.dataset.installTab === selected;
            candidate.setAttribute("aria-selected", String(isSelected));
            candidate.className = isSelected
                ? activeInstallTabClass
                : inactiveInstallTabClass;
        });
        installPanels.forEach((panel) => {
            panel.classList.toggle("hidden", panel.dataset.installPanel !== selected);
        });
    });
});
// Update copyright year dynamically
const copyrightElement = document.querySelector("#copyright-year");
if (copyrightElement) {
    copyrightElement.textContent = new Date().getFullYear().toString();
}
const scrollTopButton = document.querySelector("[data-scroll-top]");
if (scrollTopButton) {
    const updateScrollTopButton = () => {
        const shouldShow = window.scrollY > 480;
        scrollTopButton.classList.toggle("hidden", !shouldShow);
        scrollTopButton.classList.toggle("flex", shouldShow);
    };
    scrollTopButton.addEventListener("click", () => {
        window.scrollTo({ top: 0, behavior: "smooth" });
    });
    window.addEventListener("scroll", updateScrollTopButton, { passive: true });
    updateScrollTopButton();
}
