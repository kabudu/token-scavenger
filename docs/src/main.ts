const navToggle = document.querySelector<HTMLButtonElement>(
  "[data-mobile-toggle]",
);
const mobileMenu = document.querySelector<HTMLElement>("[data-mobile-nav]");

if (navToggle && mobileMenu) {
  navToggle.addEventListener("click", () => {
    const expanded = navToggle.getAttribute("aria-expanded") === "true";
    navToggle.setAttribute("aria-expanded", String(!expanded));
    mobileMenu.classList.toggle("hidden");
  });
}

const closeMenuLinks =
  document.querySelectorAll<HTMLAnchorElement>("[data-close-menu]");
closeMenuLinks.forEach((link) => {
  link.addEventListener("click", () => {
    if (mobileMenu && navToggle) {
      mobileMenu.classList.add("hidden");
      navToggle.setAttribute("aria-expanded", "false");
    }
  });
});

// Update copyright year dynamically
const copyrightElement = document.querySelector<HTMLElement>("#copyright-year");
if (copyrightElement) {
  copyrightElement.textContent = new Date().getFullYear().toString();
}

const scrollTopButton =
  document.querySelector<HTMLButtonElement>("[data-scroll-top]");

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
