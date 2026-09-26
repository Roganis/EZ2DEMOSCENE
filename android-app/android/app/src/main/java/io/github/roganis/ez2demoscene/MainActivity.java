package io.github.roganis.ez2demoscene;

import android.os.Bundle;
import com.getcapacitor.BridgeActivity;

public class MainActivity extends BridgeActivity {
    @Override
    public void onCreate(Bundle savedInstanceState) {
        // App-local plugins must be registered before the bridge starts.
        registerPlugin(SaveAsPlugin.class);
        super.onCreate(savedInstanceState);
    }
}
