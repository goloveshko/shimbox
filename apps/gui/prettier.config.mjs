// The manager's UI is plain ESM the webview reads straight off disk, so no bundler
// ever rewrites these files and nothing else in the repository says how they are
// wrapped. This is that one place. What is wrapped is the JavaScript named by the
// `check` and `format` scripts of package.json, which is the one list of files the
// build gate (scripts\build.ps1) consults rather than a glob of its own.
//
// The three choices below are not new rules imposed on the code: they are the style
// the UI was already written in, measured against it rather than guessed at
// (Prettier's own defaults are 80 columns and double quotes, which would rewrite
// every string in `ui/locales`). `printWidth` is 100 because that is the width the
// files hold to; `singleQuote` because every string literal already uses it; and
// `trailingComma` as `es5` because an argument list here ends without a comma
// after its last item.
//
// `endOfLine` is deliberately not a rule at all. Git is configured to hand out CRLF
// working trees on Windows, so the same file is LF on one machine and CRLF on the
// runner, and a formatter with an opinion about that fails every file for a reason
// no reader can see in the diff. Which ending a line ends with is git's business
// here, not this file's.
//
// The stylesheet is outside this gate, and not by oversight. Measured before
// deciding: Prettier's CSS printer rewrites 362 of app.css's 1088 lines, because
// `singleQuote` does not reach attribute selectors there (every `[type='text']`
// becomes double-quoted) and because a `color-mix(in srgb, ...)` argument list is
// split over five lines however short the line is. That is not the style the file
// was written in, and a third of its lines changing hands to prove it is a worse
// trade than the drift a gate would prevent. A person who wants the whole file in
// Prettier's shape runs `npx prettier --write ui/app.css` and reads what arrived.
export default {
  printWidth: 100,
  singleQuote: true,
  trailingComma: 'es5',
  endOfLine: 'auto',
};
