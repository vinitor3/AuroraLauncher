package com.aurora.core.api.module;

import com.aurora.core.api.AuroraServices;

public interface AuroraModuleContext {
    AuroraServices services();
    String moduleId();
}
