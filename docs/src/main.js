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
