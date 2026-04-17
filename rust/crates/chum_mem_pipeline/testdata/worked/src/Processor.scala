package com.example.processor

import scala.collection.mutable
import scala.util.{Try, Success, Failure}

/** Batch processor for streaming event data.
  * WHY: We buffer events in-memory and flush periodically rather than
  * writing each event individually — this cuts I/O by ~10x.
  */
case class Event(id: String, payload: String, timestamp: Long)

class BatchProcessor(flushThreshold: Int = 100) {
  private val buffer: mutable.ArrayBuffer[Event] = mutable.ArrayBuffer.empty

  /** Add an event to the internal buffer; auto-flushes when full. */
  def ingest(event: Event): Unit = {
    buffer.append(event)
    if (buffer.size >= flushThreshold) flush()
  }

  // NOTE: flush is idempotent — safe to call even on an empty buffer.
  def flush(): Try[Int] = Try {
    val count = buffer.size
    buffer.foreach(e => println(s"Writing event ${e.id}"))
    buffer.clear()
    count
  }

  def pending: Int = buffer.size
}

object Main extends App {
  val processor = new BatchProcessor(flushThreshold = 2)
  processor.ingest(Event("e1", "{}", System.currentTimeMillis()))
  processor.ingest(Event("e2", "{}", System.currentTimeMillis()))
  println(s"Pending after auto-flush: ${processor.pending}")
}
