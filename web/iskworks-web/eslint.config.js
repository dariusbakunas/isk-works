// The web lint gate. It starts narrow: the Rules of Hooks are an error, and
// missing effect dependencies a warning until the existing ones are fixed or
// justified, then that rule becomes an error too. The shared layer may not
// import features.
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/", "node_modules/", "playwright-report/", "test-results/"],
  },
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      parser: tseslint.parser,
    },
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
    },
  },
  {
    // The shared layer sits below features: features import it, never the
    // other way round. Tests may reach across to exercise a feature.
    files: [
      "src/api/**/*.{ts,tsx}",
      "src/components/**/*.{ts,tsx}",
      "src/hooks/**/*.{ts,tsx}",
      "src/lib/**/*.{ts,tsx}",
      "src/observability/**/*.{ts,tsx}",
    ],
    ignores: ["**/__tests__/**"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["**/features", "**/features/**"],
              message:
                "The shared layer (api, components, hooks, lib, observability) can't import features/. Move what it needs down into the shared layer instead.",
            },
          ],
        },
      ],
    },
  },
);
