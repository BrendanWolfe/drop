import assert from "node:assert/strict";
import type { Readable } from "node:stream";
import { mock, test } from "node:test";

type SourceFetcher = () => Promise<Readable | Buffer>;

const moduleMock = (
  mock as unknown as {
    module?: (
      specifier: string,
      options: {
        defaultExport?: unknown;
        namedExports?: Record<string, unknown>;
      },
    ) => void;
  }
).module?.bind(mock);

if (typeof moduleMock !== "function") {
  test(
    "transactional object import tests",
    { skip: "requires --experimental-test-module-mocks" },
    () => {},
  );
} else {
  const logger = {
    info: mock.fn(),
    warn: mock.fn(),
    error: mock.fn(),
  };

  async function defaultCreateFromSource(
    _id: string,
    sourceFetcher: SourceFetcher,
  ) {
    await sourceFetcher();
  }

  let createFromSourceImpl: (
    id: string,
    sourceFetcher: SourceFetcher,
  ) => Promise<void> = defaultCreateFromSource;

  const createFromSource = mock.fn((id: string, sourceFetcher: SourceFetcher) =>
    createFromSourceImpl(id, sourceFetcher),
  );

  moduleMock(new URL("./index.ts", import.meta.url).href, {
    defaultExport: { createFromSource },
  });
  moduleMock(new URL("../logging/index.ts", import.meta.url).href, {
    namedExports: { logger },
  });

  const { ObjectTransactionalHandler } = await import("./transactional.ts");

  let fetchImpl: (url: string) => Promise<ArrayBuffer> = async () => {
    throw new Error("No $fetch implementation was configured");
  };
  const fetchMock = mock.fn(
    (url: string, _options: unknown): Promise<ArrayBuffer> => fetchImpl(url),
  );
  Object.defineProperty(globalThis, "$fetch", {
    value: fetchMock,
    configurable: true,
    writable: true,
  });

  function encode(value: string) {
    return new TextEncoder().encode(value).buffer;
  }

  function reset() {
    createFromSourceImpl = defaultCreateFromSource;
    fetchImpl = async () => {
      throw new Error("No $fetch implementation was configured");
    };

    createFromSource.mock.resetCalls();
    logger.info.mock.resetCalls();
    logger.warn.mock.resetCalls();
    fetchMock.mock.resetCalls();
  }

  test("imports a successful arrayBuffer response", async () => {
    reset();
    fetchImpl = async () => encode("image");

    let importedSource: Readable | Buffer | undefined;
    createFromSourceImpl = async (_id, sourceFetcher) => {
      importedSource = await sourceFetcher();
    };

    const handler = new ObjectTransactionalHandler();
    const [register, pull] = handler.new({}, ["internal:read"]);
    register("https://example.com/image.png");

    await pull();

    assert.ok(importedSource instanceof Buffer);
    assert.equal(importedSource.toString(), "image");
    const fetchOptions = fetchMock.mock.calls[0]?.arguments[1] as
      | Record<string, unknown>
      | undefined;
    assert.equal(fetchOptions?.responseType, "arrayBuffer");
    assert.equal(createFromSource.mock.callCount(), 1);
    assert.equal(logger.warn.mock.callCount(), 0);
  });

  test("continues after a remote 404", async () => {
    reset();
    fetchImpl = async (url) => {
      if (url.includes("missing")) {
        throw new Error("404 Not Found");
      }
      return encode("ok");
    };

    const handler = new ObjectTransactionalHandler();
    const [register, pull] = handler.new({}, ["internal:read"]);
    register("https://example.com/missing.png");
    const goodId = register("https://example.com/ok.png");

    await pull();

    assert.equal(createFromSource.mock.callCount(), 1);
    assert.equal(createFromSource.mock.calls[0]?.arguments[0], goodId);
    assert.equal(logger.warn.mock.callCount(), 1);
  });

  test("propagates local object storage failures", async () => {
    reset();
    fetchImpl = async () => encode("ok");
    createFromSourceImpl = async () => {
      throw new Error("disk full");
    };

    const handler = new ObjectTransactionalHandler();
    const [register, pull] = handler.new({}, ["internal:read"]);
    register("https://example.com/image.png");

    await assert.rejects(pull(), /disk full/);
    assert.equal(logger.warn.mock.callCount(), 0);
  });

  test("continues after a mid-transfer download failure", async () => {
    reset();
    fetchImpl = async (url) => {
      if (url.includes("interrupted")) {
        await new Promise((resolve) => setTimeout(resolve, 1));
        throw new Error("connection reset mid-body");
      }
      return encode("ok");
    };

    const handler = new ObjectTransactionalHandler();
    const [register, pull] = handler.new({}, ["internal:read"]);
    register("https://example.com/interrupted.png");
    const goodId = register("https://example.com/ok.png");

    await pull();

    assert.equal(createFromSource.mock.callCount(), 1);
    assert.equal(createFromSource.mock.calls[0]?.arguments[0], goodId);
    assert.equal(logger.warn.mock.callCount(), 1);
  });
}
