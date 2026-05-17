// @ts-check
const assert = require("assert");
const createFromBuffer = require("@dprint/formatter").createFromBuffer;
const getPath = require("./index").getPath;

const buffer = require("fs").readFileSync(getPath());
const formatter = createFromBuffer(buffer);

const notebook = JSON.stringify({
  cells: [],
  metadata: {
    language_info: { name: "python" },
  },
  nbformat: 4,
  nbformat_minor: 2,
});

const result = formatter.formatText({
  filePath: "file.ipynb",
  fileText: notebook,
});

assert.strictEqual(result, notebook);
