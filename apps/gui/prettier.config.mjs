// The manager's UI is plain ESM the webview reads straight off disk, so no bundler
// ever rewrites these files and nothing else in the repository says how they are
// wrapped. This is that one place.
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
export default {
  printWidth: 100,
  singleQuote: true,
  trailingComma: 'es5',
  endOfLine: 'auto',
};
