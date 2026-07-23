@testitem "PkgB double works" begin
    using Test
    include("../src/PkgB.jl")
    using .PkgB
    @test PkgB.double(3) == 6
    @test PkgB.double(0) == 0
end

@testitem "PkgB square works" begin
    using Test
    include("../src/PkgB.jl")
    using .PkgB
    @test PkgB.square(4) == 16
    @test PkgB.square(-3) == 9
end