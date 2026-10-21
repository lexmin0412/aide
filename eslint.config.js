// ESLint 9 flat config for the aide frontend.
//
// Prettier is intentionally not part of this setup: the repository has no
// formatting baseline yet and adding it would trigger a repo-wide reformat.
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import i18next from "eslint-plugin-i18next";

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/**",
      "coverage/**",
      // Local pnpm store used by sandboxed environments (see .gitignore).
      ".pnpm-store/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat.recommended,
  jsxA11y.flatConfigs.recommended,
  {
    plugins: { i18next },
    rules: {
      // TODO: enable once user-facing strings are extracted to i18next. The
      // app still hard-codes many UI strings, so this would flood the report.
      "i18next/no-literal-string": "off",
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      // Hooks correctness is blocking. The compiler-oriented checks
      // (react-hooks v7) are blocking too: the codebase was cleaned up so
      // refs are only touched in effects/handlers, effects only update state
      // after awaits, and manual memoization is preserved.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
      "react-hooks/refs": "error",
      "react-hooks/set-state-in-effect": "error",
      "react-hooks/preserve-manual-memoization": "error",
      // Empty catch blocks must carry an explanatory comment.
      "no-empty": "error",
      // Tauri IPC boundaries are typed; `any` is no longer needed.
      "@typescript-eslint/no-explicit-any": "error",
      // Accessibility: clickable elements need semantic roles and keyboard
      // handlers, autofocus is avoided, and tabIndex stays on interactive
      // elements only.
      "jsx-a11y/no-static-element-interactions": "error",
      "jsx-a11y/click-events-have-key-events": "error",
      "jsx-a11y/no-autofocus": "error",
      "jsx-a11y/no-noninteractive-tabindex": "error",
    },
  },
);
