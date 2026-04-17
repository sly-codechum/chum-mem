package com.example.app;

import java.util.List;
import java.util.stream.Collectors;

/**
 * Main application entry point.
 * WHY: We use a builder pattern here so integration tests can override
 * individual services without subclassing.
 */
public class App {

    private final List<String> plugins;

    public App(List<String> plugins) {
        this.plugins = plugins;
    }

    /** Load and validate all registered plugins. */
    public void initialize() {
        List<String> valid = plugins.stream()
                .filter(p -> !p.isBlank())
                .collect(Collectors.toList());
        System.out.println("Loaded " + valid.size() + " plugins");
        valid.forEach(this::activate);
    }

    // NOTE: Activation order matters — network plugins must come first.
    private void activate(String plugin) {
        System.out.println("Activating: " + plugin);
    }

    public static void main(String[] args) {
        App app = new App(List.of("network", "storage", "compute"));
        app.initialize();
    }
}
