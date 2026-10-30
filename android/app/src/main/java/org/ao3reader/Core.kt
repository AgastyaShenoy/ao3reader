package org.ao3reader

import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors

class CoreError(message: String) : Exception(message)

/**
 * The Rust core (libao3core.so). Every call is `method` + JSON arguments → JSON result.
 * Quick calls run on the UI thread; network calls go through [background].
 */
object Core {
    init {
        System.loadLibrary("ao3core")
    }

    @JvmStatic
    private external fun call(method: String, arg: String): String

    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    fun req(method: String, arg: JSONObject = JSONObject()): Any? {
        val res = JSONObject(call(method, arg.toString()))
        if (res.has("err")) throw CoreError(res.getString("err"))
        return res.opt("ok")
    }

    fun obj(method: String, arg: JSONObject = JSONObject()): JSONObject = req(method, arg) as JSONObject
    fun arr(method: String, arg: JSONObject = JSONObject()): JSONArray = req(method, arg) as JSONArray

    /** Runs a (possibly slow, network) call off the UI thread and delivers the result on it. */
    fun background(method: String, arg: JSONObject, done: (Result<Any?>) -> Unit) {
        worker.execute {
            val r = runCatching { req(method, arg) }
            main.post { done(r) }
        }
    }
}

fun json(vararg pairs: Pair<String, Any?>): JSONObject = JSONObject().apply { pairs.forEach { (k, v) -> put(k, v) } }

fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }
fun JSONArray.strings(): List<String> = (0 until length()).map { getString(it) }
