import { generateChangeLog } from "jsr:@dprint/automation@0.10.3";

const version = Deno.args[0];
const changelog = await generateChangeLog({
  versionTo: version,
});
const text = `## Changes

${changelog}

## Install

[Install](https://dprint.dev/install/) and [setup](https://dprint.dev/setup/) dprint.

Then in your project's directory with a dprint.json file, run:

\`\`\`shellsession
dprint config add jupyter
\`\`\`

Then add some additional formatting plugins to format the code blocks with. For example:

\`\`\`shellsession
dprint config add typescript
dprint config add markdown
dprint config add ruff
\`\`\`

## JS Formatting API

* [JS Formatter](https://github.com/dprint/js-formatter) - Browser/Deno and Node
* [npm package](https://www.npmjs.com/package/@dprint/jupyter)
`;

console.log(text);
