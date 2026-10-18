/*
The purpose of this class is to hold references to remote objects (like images) until they're actually needed
This is used as a utility in metadata handling, so we only fetch the objects if we're actually creating a database record.
*/
import type { Readable } from "node:stream";
import { randomUUID } from "node:crypto";
import objectHandler from "./index.ts";
import type { TaskRunContext } from "../tasks";
import { logger } from "../logging/index.ts";

export type TransactionDataType = string | Readable | Buffer;
type TransactionTable = Map<string, TransactionDataType>; // ID to data
type GlobalTransactionRecord = Map<string, TransactionTable>; // Transaction ID to table

export type Register = (url: TransactionDataType) => string;
export type Pull = () => Promise<void>;
export type Dump = () => void;

export class ObjectTransactionalHandler {
  private record: GlobalTransactionRecord = new Map();

  new(
    metadata: { [key: string]: string },
    permissions: Array<string>,
    context?: TaskRunContext,
  ): [Register, Pull, Dump] {
    const transactionId = randomUUID();

    this.record.set(transactionId, new Map());

    const register = (data: TransactionDataType) => {
      const objectId = randomUUID();
      this.record.get(transactionId)?.set(objectId, data);

      return objectId;
    };

    const pull = async () => {
      const transaction = this.record.get(transactionId);
      if (!transaction) return;

      let progress = 0;
      const increment = 100 / transaction.size;
      const taskLogger = context?.logger ?? logger;

      for (const [id, data] of transaction) {
        let source: Readable | Buffer;

        if (typeof data === "string") {
          taskLogger.info(`Importing object from "${data}"`);
          try {
            const remoteSource = await $fetch<ArrayBuffer>(data, {
              responseType: "arrayBuffer",
            });
            source = Buffer.from(remoteSource);
          } catch (e) {
            taskLogger.warn(
              `Failed to download remote object from "${data}", skipping: ${e}`,
            );
            progress += increment;
            context?.progress(progress);
            continue;
          }
        } else {
          taskLogger.info(`Importing raw object...`);
          source = data;
        }

        await objectHandler.createFromSource(
          id,
          async () => source,
          metadata,
          permissions,
        );

        progress += increment;
        context?.progress(progress);
      }
    };

    const dump = () => {
      this.record.delete(transactionId);
    };

    return [register, pull, dump];
  }
}
