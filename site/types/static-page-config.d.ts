// Next 16's generated PagesPageConfig omits the still-supported
// unstable_runtimeJS option; optional runtime keeps that export assignable
// without setting a runtime for static pages.
declare global {
  type StaticExportPageConfig = {
    runtime?: undefined;
    unstable_runtimeJS: false;
  };
}

export {};
