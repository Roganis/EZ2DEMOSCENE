package io.github.roganis.ez2demoscene;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.util.Base64;
import androidx.activity.result.ActivityResult;
import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.ActivityCallback;
import com.getcapacitor.annotation.CapacitorPlugin;
import java.io.OutputStream;

/**
 * "Save as…" for the web app: opens Android's document picker
 * (ACTION_CREATE_DOCUMENT) so the user chooses where the file goes, then
 * writes the bytes there. Unlike the share sheet, this works for file types
 * Android doesn't know, such as .ez2pack.
 */
@CapacitorPlugin(name = "Ez2SaveAs")
public class SaveAsPlugin extends Plugin {

    /** Bytes waiting for the user to pick a location. */
    private byte[] pending;

    @PluginMethod
    public void save(PluginCall call) {
        String name = call.getString("name", "file");
        String data = call.getString("data");
        String mime = call.getString("mime", "application/octet-stream");
        if (data == null) {
            call.reject("No data to save");
            return;
        }
        try {
            pending = Base64.decode(data, Base64.DEFAULT);
        } catch (IllegalArgumentException e) {
            call.reject("Bad data: " + e.getMessage());
            return;
        }
        Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType(mime);
        intent.putExtra(Intent.EXTRA_TITLE, name);
        startActivityForResult(call, intent, "onPicked");
    }

    @ActivityCallback
    private void onPicked(PluginCall call, ActivityResult result) {
        byte[] bytes = pending;
        pending = null;
        if (call == null) {
            return;
        }
        JSObject out = new JSObject();
        Intent data = result.getData();
        if (result.getResultCode() != Activity.RESULT_OK || data == null || data.getData() == null || bytes == null) {
            out.put("saved", false);
            call.resolve(out);
            return;
        }
        Uri uri = data.getData();
        try (OutputStream stream = getContext().getContentResolver().openOutputStream(uri, "wt")) {
            if (stream == null) {
                call.reject("Could not open the chosen file");
                return;
            }
            stream.write(bytes);
            out.put("saved", true);
            out.put("uri", uri.toString());
            call.resolve(out);
        } catch (Exception e) {
            call.reject("Could not save: " + e.getMessage());
        }
    }
}
