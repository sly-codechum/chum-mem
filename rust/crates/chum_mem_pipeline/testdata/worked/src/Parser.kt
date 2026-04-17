package com.example.parser

import java.io.File
import java.nio.charset.Charset

/**
 * Lightweight token-based parser for config files.
 * WHY: We avoid a full grammar library because our config format
 * is simple enough that split + map covers every case.
 */
data class Token(val key: String, val value: String, val line: Int)

class ConfigParser(private val charset: Charset = Charsets.UTF_8) {

    // NOTE: Blank lines and comments (starting with #) are skipped.
    fun parse(file: File): List<Token> {
        val lines = file.readLines(charset)
        return lines.mapIndexedNotNull { idx, raw ->
            tokenize(raw.trim(), idx + 1)
        }
    }

    private fun tokenize(line: String, lineNum: Int): Token? {
        if (line.isBlank() || line.startsWith("#")) return null
        val parts = line.split("=", limit = 2)
        require(parts.size == 2) { "Malformed line $lineNum: $line" }
        return Token(parts[0].trim(), parts[1].trim(), lineNum)
    }
}

fun main() {
    val parser = ConfigParser()
    val tokens = parser.parse(File("app.conf"))
    tokens.forEach { println("${it.key} -> ${it.value}") }
    println("Total entries: ${tokens.size}")
}
