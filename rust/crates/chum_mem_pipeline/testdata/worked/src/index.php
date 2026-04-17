<?php

declare(strict_types=1);

require_once __DIR__ . '/vendor/autoload.php';

use App\Http\Router;
use App\Services\CacheService;

// WHY: We register routes as closures so the router stays decoupled
// from any particular controller class hierarchy.

class Application
{
    private Router $router;
    private CacheService $cache;

    public function __construct(Router $router, CacheService $cache)
    {
        $this->router = $router;
        $this->cache = $cache;
    }

    /** Boot the application and register default routes. */
    public function boot(): void
    {
        $this->router->get('/status', function () {
            return ['status' => 'ok', 'cached' => $this->cache->has('init')];
        });
        $this->cache->set('init', true);
        echo "Application booted\n";
    }

    // NOTE: Dispatch returns null for unmatched routes — the caller
    // is responsible for sending a 404 response.
    public function dispatch(string $method, string $path): ?array
    {
        return $this->router->resolve($method, $path);
    }
}

$app = new Application(new Router(), new CacheService());
$app->boot();
$result = $app->dispatch('GET', '/status');
echo json_encode($result) . "\n";
